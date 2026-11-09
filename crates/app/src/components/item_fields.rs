//! The work item dialog's richer fields: the searchable parent combobox, the tag input with the
//! team's tags as suggestions, the description editor (HTML, `contenteditable`), and the state,
//! person and date pickers. Each takes its value and reports changes; the dialog owns the draft.

use dioxus::prelude::*;
use kanban_core::{CardId, Person, Settings, StateCategory, Workspace};
use time::Date;

use crate::components::icons;
use crate::components::pickers::{Calendar, long_date};
use crate::components::widgets::{Avatar, Icon, StatePill, today};
use crate::state::use_app;

/// Menus with more entries than this get a filter box.
const FILTER_FROM: usize = 8;
/// Tag suggestions shown at once.
const TAG_SUGGESTIONS: usize = 6;

/// A possible parent in [`ParentPicker`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParentOption {
    pub id: CardId,
    pub title: String,
    /// Hex without `#`.
    pub type_color: String,
    /// Second line: area and child count, or what it hangs under.
    pub context: String,
    pub state: String,
    pub category: StateCategory,
    pub state_color: String,
    /// Shown in the "Recently changed" group.
    pub recent: bool,
}

fn highlight(title: &str, q: &str) -> (String, String, String) {
    let lower = title.to_lowercase();
    match lower.find(q) {
        // Only when lowercasing kept the byte positions (ASCII and most Latin text).
        Some(at) if !q.is_empty() && lower.len() == title.len() => (
            title[..at].to_owned(),
            title[at..at + q.len()].to_owned(),
            title[at + q.len()..].to_owned(),
        ),
        _ => (title.to_owned(), String::new(), String::new()),
    }
}

/// The picked parent as a small card with "Change", or a search box listing the candidates
/// (by title or id; ↑/↓, Enter, Esc).
#[component]
pub fn ParentPicker(
    options: Vec<ParentOption>,
    value: Option<CardId>,
    /// Plural for the count and the empty text, e.g. "epics".
    noun: String,
    onpick: EventHandler<Option<CardId>>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| false);
    let mut changing = use_signal(|| false);
    let mut active = use_signal(|| 0usize);
    let picked = value.and_then(|v| options.iter().find(|o| o.id == v).cloned());
    if let Some(p) = picked.filter(|_| !changing()) {
        return rsx! {
            div { class: "picked",
                span { class: "tdot", style: "--tc: #{p.type_color}" }
                span { class: "id", "{p.id}" }
                span { class: "t", b { "{p.title}" } small { "{p.context}" } }
                StatePill { state: p.state.clone(), category: p.category, color: p.state_color.clone() }
                button {
                    class: "ghostbtn",
                    r#type: "button",
                    onclick: move |_| {
                        changing.set(true);
                        open.set(true);
                        query.set(String::new());
                        active.set(0);
                    },
                    "Change"
                }
            }
        };
    }

    let q = query().trim().trim_start_matches('#').to_lowercase();
    let matches = |o: &&ParentOption| {
        q.is_empty() || o.title.to_lowercase().contains(&q) || o.id.to_string().starts_with(&q)
    };
    let groups: Vec<(String, Vec<ParentOption>)> = if q.is_empty() {
        let recent: Vec<ParentOption> = options.iter().filter(|o| o.recent).cloned().collect();
        let rest: Vec<ParentOption> = options.iter().filter(|o| !o.recent).cloned().collect();
        [
            ("Recently changed".to_owned(), recent),
            (format!("All {noun} · {}", rest.len()), rest),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .collect()
    } else {
        let found: Vec<ParentOption> = options.iter().filter(matches).cloned().collect();
        if found.is_empty() {
            Vec::new()
        } else {
            vec![(format!("Matches · {}", found.len()), found)]
        }
    };
    let flat: Vec<CardId> = groups
        .iter()
        .flat_map(|(_, v)| v.iter().map(|o| o.id))
        .collect();
    let n = flat.len();
    let count = if q.is_empty() {
        format!("{} {noun}", options.len())
    } else {
        format!("{n} found")
    };
    let current = active().min(n.saturating_sub(1));
    let mut pick = move |id: CardId| {
        onpick.call(Some(id));
        changing.set(false);
        open.set(false);
        query.set(String::new());
    };
    let mut cancel = move || {
        open.set(false);
        changing.set(false);
        query.set(String::new());
    };
    let flat_keys = flat.clone();
    let mut index = 0usize;
    rsx! {
        div { class: "combo anchor",
            span { class: "ico-l", Icon { svg: icons::SEARCH } }
            input {
                class: "in",
                r#type: "text",
                role: "combobox",
                aria_expanded: "{open()}",
                aria_controls: "parent-lb",
                aria_autocomplete: "list",
                aria_label: "Parent",
                placeholder: "Search by ID or title",
                value: "{query}",
                onmounted: move |e| async move {
                    if changing() {
                        let _ = e.set_focus(true).await;
                    }
                },
                onfocus: move |_| open.set(true),
                oninput: move |e| {
                    query.set(e.value());
                    active.set(0);
                    open.set(true);
                },
                onkeydown: move |e| match e.key() {
                    Key::ArrowDown if n > 0 => {
                        open.set(true);
                        active.set((current + 1) % n);
                    }
                    Key::ArrowUp if n > 0 => active.set((current + n - 1) % n),
                    Key::Enter => {
                        if let Some(id) = flat_keys.get(current).copied().filter(|_| open()) {
                            pick(id);
                        }
                    }
                    Key::Escape if open() || changing() => {
                        e.stop_propagation();
                        cancel();
                    }
                    _ => {}
                },
            }
            span { class: "kbd", "{count}" }
            if open() {
                div { class: "popover-backdrop", onclick: move |_| cancel() }
                div { class: "menu",
                    div { class: "scroll", id: "parent-lb", role: "listbox", aria_label: "Parent",
                        for (name, items) in groups {
                            div { key: "{name}", class: "grp", role: "presentation", "{name}" }
                            for o in items {
                                {
                                    let i = index;
                                    index += 1;
                                    let (pre, hit, post) = highlight(&o.title, &q);
                                    let id = o.id;
                                    rsx! {
                                        button {
                                            key: "{o.id}",
                                            class: if i == current { "opt active" } else { "opt" },
                                            r#type: "button",
                                            role: "option",
                                            aria_selected: "{i == current}",
                                            onmouseenter: move |_| active.set(i),
                                            onclick: move |_| pick(id),
                                            span { class: "tdot", style: "--tc: #{o.type_color}" }
                                            span { class: "id", "{o.id}" }
                                            span { class: "t",
                                                b { "{pre}" if !hit.is_empty() { mark { "{hit}" } } "{post}" }
                                                small { "{o.context}" }
                                            }
                                            StatePill { state: o.state.clone(), category: o.category, color: o.state_color.clone() }
                                        }
                                    }
                                }
                            }
                        }
                        if n == 0 {
                            div { class: "empty",
                                if q.is_empty() { "No open {noun} on this board." } else { "Nothing matches “{query}”. Try an ID, or clear the search." }
                            }
                        }
                    }
                    div { class: "lb-foot",
                        span { kbd { "↑" } kbd { "↓" } "move" }
                        span { kbd { "Enter" } "pick" }
                        span { kbd { "Esc" } "close" }
                        span { class: "end", "Closed {noun} are hidden" }
                    }
                }
            }
        }
    }
}

/// Tags as removable chips plus a text box. Suggests the team's tags (most used first) and
/// offers to create a new one. Enter or `,` adds; Backspace on an empty box removes the last.
#[component]
pub fn TagInput(
    tags: Vec<String>,
    /// Edit: the saved tags, so new ones are marked.
    original: Option<Vec<String>>,
    known: Vec<(String, usize)>,
    onchange: EventHandler<Vec<String>>,
) -> Element {
    let mut q = use_signal(String::new);
    let mut open = use_signal(|| false);
    let mut active = use_signal(|| None::<usize>);
    let text = q().trim().to_lowercase();
    let has = |t: &str| tags.iter().any(|x| x.eq_ignore_ascii_case(t));
    let sugs: Vec<(String, usize)> = known
        .iter()
        .filter(|(t, _)| !has(t) && (text.is_empty() || t.to_lowercase().contains(&text)))
        .take(TAG_SUGGESTIONS)
        .cloned()
        .collect();
    let exists = known.iter().any(|(t, _)| t.to_lowercase() == text) || has(&text);
    let can_create = !text.is_empty() && !exists;
    let n = sugs.len() + usize::from(can_create);
    let show = open() && n > 0;

    let add = {
        let tags = tags.clone();
        move |t: String| {
            let mut next = tags.clone();
            kanban_core::item::add_tag(&mut next, &t);
            onchange.call(next);
            q.set(String::new());
            active.set(None);
        }
    };
    // Enter: the highlighted suggestion, else the known spelling of what was typed, else new.
    let commit = {
        let sugs = sugs.clone();
        let known = known.clone();
        let mut add = add.clone();
        move || {
            let typed = q.peek().trim().to_owned();
            match *active.peek() {
                Some(i) if i < sugs.len() => add(sugs[i].0.clone()),
                _ if !typed.is_empty() => {
                    let spelled = known
                        .iter()
                        .find(|(t, _)| t.eq_ignore_ascii_case(&typed))
                        .map_or(typed, |(t, _)| t.clone());
                    add(spelled);
                }
                _ => {}
            }
        }
    };
    let last_removed = {
        let tags = tags.clone();
        move || {
            let mut next = tags.clone();
            next.pop();
            onchange.call(next);
        }
    };
    let title = if text.is_empty() {
        "Used by the team"
    } else {
        "Matching tags"
    };
    rsx! {
        div { class: "anchor",
            div { class: "tagbox",
                for t in tags.clone() {
                    {
                        let added = original.as_ref().is_some_and(|o| !o.iter().any(|x| x.eq_ignore_ascii_case(&t)));
                        let tags = tags.clone();
                        let remove = t.clone();
                        rsx! {
                            span { key: "{t}", class: if added { "tchip added" } else { "tchip" },
                                "{t}"
                                button {
                                    r#type: "button",
                                    aria_label: "Remove tag {t}",
                                    onclick: move |_| {
                                        onchange.call(tags.iter().filter(|x| **x != remove).cloned().collect());
                                    },
                                    Icon { svg: icons::CLOSE_12 }
                                }
                            }
                        }
                    }
                }
                input {
                    r#type: "text",
                    role: "combobox",
                    aria_label: "Add tag",
                    aria_expanded: "{show}",
                    aria_autocomplete: "list",
                    placeholder: "Add tag",
                    value: "{q}",
                    onfocus: move |_| open.set(true),
                    oninput: {
                        let mut commit = commit.clone();
                        move |e: FormEvent| {
                            let v = e.value();
                            if v.ends_with(',') {
                                q.set(v.trim_end_matches(',').to_owned());
                                commit();
                            } else {
                                q.set(v);
                                active.set(None);
                                open.set(true);
                            }
                        }
                    },
                    onkeydown: {
                        let mut commit = commit.clone();
                        let last_removed = last_removed.clone();
                        let empty_tags = tags.is_empty();
                        move |e: KeyboardEvent| match e.key() {
                            Key::Enter => commit(),
                            Key::ArrowDown if n > 0 => {
                                open.set(true);
                                active.set(Some(active().map_or(0, |i| (i + 1) % n)));
                            }
                            Key::ArrowUp if n > 0 => active.set(Some(active().map_or(n - 1, |i| (i + n - 1) % n))),
                            Key::Backspace if q.peek().is_empty() && !empty_tags => last_removed(),
                            Key::Escape if open() => {
                                e.stop_propagation();
                                open.set(false);
                            }
                            _ => {}
                        }
                    },
                }
            }
            if show {
                div { class: "popover-backdrop", onclick: move |_| open.set(false) }
                div { class: "menu", role: "listbox", aria_label: "Tag suggestions",
                    if !sugs.is_empty() { div { class: "grp", role: "presentation", "{title}" } }
                    for (i, (t, count)) in sugs.iter().cloned().enumerate() {
                        {
                            let mut add = add.clone();
                            let pick = t.clone();
                            rsx! {
                                button {
                                    key: "{t}",
                                    class: if active() == Some(i) { "item active" } else { "item" },
                                    r#type: "button",
                                    role: "option",
                                    aria_selected: "{active() == Some(i)}",
                                    onclick: move |_| add(pick.clone()),
                                    span { class: "grow", "{t}" }
                                    span { class: "mono", "{count}" }
                                }
                            }
                        }
                    }
                    if can_create {
                        {
                            let mut add = add.clone();
                            let typed = q().trim().to_owned();
                            let i = sugs.len();
                            rsx! {
                                button {
                                    class: if active() == Some(i) { "item create active" } else { "item create" },
                                    r#type: "button",
                                    role: "option",
                                    aria_selected: "{active() == Some(i)}",
                                    onclick: move |_| add(typed.clone()),
                                    "Create “{typed}”"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Wires the toolbar (once per mount). `execCommand` is deprecated but still the only built-in
/// way to edit `contenteditable` HTML, and every webview the app runs on supports it.
const EDITOR_JS: &str = r#"
const root = document.getElementById(__ID__);
if (!root || root.dataset.wired) return;
root.dataset.wired = '1';
const area = root.querySelector('.rte-area');
const bar = root.querySelector('.rte-bar');
const esc = (s) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
bar.addEventListener('mousedown', (e) => { if (e.target.closest('button')) e.preventDefault(); });
bar.addEventListener('click', (e) => {
  const b = e.target.closest('button[data-cmd]');
  if (!b) return;
  area.focus();
  const sel = window.getSelection();
  const text = sel ? sel.toString() : '';
  switch (b.dataset.cmd) {
    case 'bold': case 'italic': case 'insertUnorderedList':
      document.execCommand(b.dataset.cmd); break;
    case 'check':
      document.execCommand('insertUnorderedList');
      document.execCommand('insertText', false, '[ ] '); break;
    case 'link': {
      const url = window.prompt('Link address', 'https://');
      if (!url || url === 'https://') break;
      if (text) document.execCommand('createLink', false, url);
      else document.execCommand('insertHTML', false, '<a href="' + esc(url) + '">' + esc(url) + '</a>');
      break;
    }
    case 'code':
      document.execCommand('insertHTML', false, '<code>' + esc(text || 'code') + '</code>'); break;
    case 'criteria': {
      const r = document.createRange();
      r.selectNodeContents(area);
      r.collapse(false);
      sel.removeAllRanges();
      sel.addRange(r);
      document.execCommand('insertHTML', false, '<h4>Acceptance criteria</h4><ul><li><br></li></ul>');
      break;
    }
  }
});
"#;

/// The editor's current HTML (read when saving, not on every keystroke).
pub async fn read_editor(id: &str) -> Option<String> {
    let js = format!(
        "const a = document.querySelector('#{id} .rte-area'); return a ? a.innerHTML : null;"
    );
    document::eval(&js)
        .join::<Option<String>>()
        .await
        .ok()
        .flatten()
}

/// Description editor. Starts from `initial` (sanitised HTML) and owns its content after that;
/// remount it (a new `key`) to load other content.
#[component]
pub fn DescriptionEditor(
    id: String,
    initial: String,
    /// Offer "Add acceptance criteria".
    criteria: bool,
    labelledby: String,
    ondirty: EventHandler<()>,
) -> Element {
    let script = EDITOR_JS.replace("__ID__", &format!("{id:?}"));
    let tools: [(&str, &str, &'static str); 6] = [
        ("bold", "Bold", icons::BOLD),
        ("italic", "Italic", icons::ITALIC),
        ("insertUnorderedList", "Bulleted list", icons::BULLETS),
        ("check", "Checklist", icons::CHECKLIST),
        ("link", "Link", icons::LINK),
        ("code", "Code", icons::CODE),
    ];
    rsx! {
        div {
            class: "rte",
            id: "{id}",
            onmounted: move |_| {
                let _ = document::eval(&script);
            },
            div { class: "rte-bar", role: "toolbar", aria_label: "Formatting",
                for (cmd, label, svg) in tools.iter().take(2).copied() {
                    button { key: "{cmd}", r#type: "button", tabindex: "-1", "data-cmd": cmd, "data-tip": label, aria_label: label, Icon { svg } }
                }
                span { class: "sep" }
                for (cmd, label, svg) in tools.iter().skip(2).copied() {
                    button { key: "{cmd}", r#type: "button", tabindex: "-1", "data-cmd": cmd, "data-tip": label, aria_label: label, Icon { svg } }
                }
                if criteria {
                    button { class: "tpl", r#type: "button", tabindex: "-1", "data-cmd": "criteria", "Add acceptance criteria" }
                }
            }
            div {
                class: "rte-area",
                contenteditable: "true",
                role: "textbox",
                aria_multiline: "true",
                aria_labelledby: "{labelledby}",
                "data-placeholder": "Add context, links and acceptance criteria.",
                dangerous_inner_html: "{initial}",
                oninput: move |_| ondirty.call(()),
            }
        }
    }
}

/// A state in the menu: name, category, color, note.
type StateEntry = (String, StateCategory, String, String);

/// The states the dialog offers for `work_item_type`, with the kanban column each puts the card
/// in. Items on a kanban get only the states of its visible columns (hidden columns included in
/// Settings stay hidden here too); tasks and top-level items get every state of their type.
pub fn offered_states(
    ws: &Workspace,
    settings: &Settings,
    work_item_type: &str,
) -> Vec<(String, Option<String>)> {
    match kanban_core::filters::board_states(ws, settings, work_item_type) {
        Some(states) => states.into_iter().map(|(s, c)| (s, Some(c))).collect(),
        None => ws
            .states
            .states(work_item_type)
            .into_iter()
            .map(|s| (s, None))
            .collect(),
    }
}

const STATE_GROUPS: [(StateCategory, &str); 5] = [
    (StateCategory::Proposed, "Not started"),
    (StateCategory::InProgress, "In progress"),
    (StateCategory::Resolved, "Resolved"),
    (StateCategory::Completed, "Finished"),
    (StateCategory::Removed, "Removed"),
];

/// State as a pill; the menu groups the type's states by category and says which column each
/// one puts the card in.
#[component]
pub fn StateMenu(
    work_item_type: String,
    value: String,
    /// Edit: the saved state ("current"); new: the default is marked instead.
    original: Option<String>,
    onpick: EventHandler<String>,
) -> Element {
    let st = use_app();
    let mut open = use_signal(|| false);
    let ws_guard = st.workspace.read();
    let Some(ws) = ws_guard.as_ref() else {
        return rsx! {};
    };
    let offered = offered_states(ws, &st.settings.read(), &work_item_type);
    let default = offered.first().map(|(s, _)| s.clone()).unwrap_or_default();
    // The saved state stays visible even when its column is hidden.
    let mut states: Vec<(String, Option<String>)> = offered.clone();
    for keep in [Some(&value), original.as_ref()].into_iter().flatten() {
        if !keep.is_empty() && !states.iter().any(|(s, _)| s == keep) {
            states.push((keep.clone(), None));
        }
    }
    let entry = |(s, column): &(String, Option<String>)| {
        let note = if original.as_ref() == Some(s) {
            "current".to_owned()
        } else if original.is_none() && *s == default {
            "default".to_owned()
        } else {
            column
                .as_ref()
                .map(|c| format!("→ {c}"))
                .unwrap_or_default()
        };
        (
            s.clone(),
            ws.states.category(&work_item_type, s),
            ws.states.color(&work_item_type, s),
            note,
        )
    };
    let groups: Vec<(&str, Vec<StateEntry>)> = STATE_GROUPS
        .iter()
        .map(|(cat, name)| {
            (
                *name,
                states
                    .iter()
                    .filter(|(s, _)| ws.states.category(&work_item_type, s) == *cat)
                    .map(entry)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let category = ws.states.category(&work_item_type, &value);
    let color = ws.states.color(&work_item_type, &value);
    drop(ws_guard);
    rsx! {
        div { class: "anchor",
            button {
                class: if open() { "selectlike state-sel open" } else { "selectlike state-sel" },
                r#type: "button",
                aria_haspopup: "listbox",
                aria_expanded: "{open()}",
                aria_label: "Status: {value}",
                onclick: move |_| open.toggle(),
                StatePill { state: value.clone(), category, color }
                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
            }
            if open() {
                div { class: "popover-backdrop", onclick: move |_| open.set(false) }
                div {
                    class: "menu",
                    role: "listbox",
                    aria_label: "Status",
                    onkeydown: move |e| if e.key() == Key::Escape {
                        e.stop_propagation();
                        open.set(false);
                    },
                    div { class: "scroll",
                        for (name, items) in groups {
                            div { key: "{name}", class: "grp", role: "presentation", "{name}" }
                            for (s, _, c, note) in items {
                                button {
                                    key: "{s}",
                                    class: if s == value { "item on" } else { "item" },
                                    style: "--sc: #{c}",
                                    r#type: "button",
                                    role: "option",
                                    aria_selected: "{s == value}",
                                    onclick: {
                                        let s = s.clone();
                                        move |_| {
                                            onpick.call(s.clone());
                                            open.set(false);
                                        }
                                    },
                                    i {}
                                    span { class: "grow", "{s}" }
                                    small { "{note}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Assignee with avatars; "Unassigned" at the end.
#[component]
pub fn PersonMenu(
    value: String,
    people: Vec<Person>,
    me: Option<Person>,
    onpick: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut filter = use_signal(String::new);
    let is_me = |p: &Person| me.as_ref().is_some_and(|m| m.unique_name == p.unique_name);
    let name = |p: &Person| {
        if is_me(p) {
            format!("Me ({})", p.display_name)
        } else {
            p.display_name.clone()
        }
    };
    let current = people.iter().find(|p| p.unique_name == value).cloned();
    let f = filter().trim().to_lowercase();
    let shown: Vec<(Person, String, bool)> = people
        .iter()
        .filter(|p| f.is_empty() || p.display_name.to_lowercase().contains(&f))
        .map(|p| (p.clone(), name(p), is_me(p)))
        .collect();
    let many = people.len() > FILTER_FROM;
    let label = current.as_ref().map(name);
    rsx! {
        div { class: "anchor",
            button {
                class: if open() { "selectlike open" } else { "selectlike" },
                r#type: "button",
                aria_haspopup: "listbox",
                aria_expanded: "{open()}",
                onclick: move |_| {
                    open.toggle();
                    filter.set(String::new());
                },
                if let Some(p) = current { Avatar { person: p, small: true } }
                match label {
                    Some(l) => rsx! { span { class: "grow", "{l}" } },
                    None => rsx! { span { class: "grow ph", "Unassigned" } },
                }
                span { class: "chev", Icon { svg: icons::CHEVRON_DOWN } }
            }
            if open() {
                div { class: "popover-backdrop", onclick: move |_| open.set(false) }
                div {
                    class: "menu",
                    role: "listbox",
                    aria_label: "Assigned to",
                    onkeydown: move |e| if e.key() == Key::Escape {
                        e.stop_propagation();
                        open.set(false);
                    },
                    if many {
                        label { class: "srch",
                            Icon { svg: icons::SEARCH }
                            input {
                                r#type: "text",
                                placeholder: "Filter people",
                                aria_label: "Filter people",
                                value: "{filter}",
                                onmounted: move |e| async move { let _ = e.set_focus(true).await; },
                                oninput: move |e| filter.set(e.value()),
                            }
                        }
                    }
                    div { class: "scroll",
                        for (p, n, mine) in shown {
                            {
                                let un = p.unique_name.clone();
                                rsx! {
                                    button {
                                        key: "{p.unique_name}",
                                        class: if p.unique_name == value { "item on" } else { "item" },
                                        r#type: "button",
                                        role: "option",
                                        aria_selected: "{p.unique_name == value}",
                                        onclick: move |_| {
                                            onpick.call(un.clone());
                                            open.set(false);
                                        },
                                        Avatar { person: p.clone(), small: true }
                                        span { class: "grow", "{n}" }
                                        if mine { small { "you" } }
                                    }
                                }
                            }
                        }
                        button {
                            class: if value.is_empty() { "item on" } else { "item" },
                            r#type: "button",
                            role: "option",
                            aria_selected: "{value.is_empty()}",
                            onclick: move |_| {
                                onpick.call(String::new());
                                open.set(false);
                            },
                            span { class: "grow", "Unassigned" }
                        }
                    }
                }
            }
        }
    }
}

/// A date with a calendar popover, plus "+1 week" (moves the date a week later, or sets it a week
/// from today when empty) and "Clear".
#[component]
pub fn DateField(
    value: Option<Date>,
    placeholder: String,
    onpick: EventHandler<Option<Date>>,
) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        div { class: "anchor",
            button {
                class: if open() { "selectlike datebtn open" } else { "selectlike datebtn" },
                r#type: "button",
                aria_haspopup: "dialog",
                aria_expanded: "{open()}",
                onclick: move |_| open.toggle(),
                Icon { svg: icons::CALENDAR_16 }
                match value {
                    Some(d) => rsx! { span { class: "grow", "{long_date(d)}" } },
                    None => rsx! { span { class: "grow ph", "{placeholder}" } },
                }
            }
            if open() {
                div { class: "popover-backdrop", onclick: move |_| open.set(false) }
                div {
                    class: "menu",
                    onkeydown: move |e| if e.key() == Key::Escape {
                        e.stop_propagation();
                        open.set(false);
                    },
                    Calendar {
                        value,
                        max: None,
                        onpick: move |d| {
                            onpick.call(Some(d));
                            open.set(false);
                        },
                    }
                }
            }
        }
        div { class: "quick",
            button {
                class: "qchip",
                r#type: "button",
                onclick: move |_| onpick.call(Some(value.unwrap_or_else(today) + time::Duration::days(7))),
                "+1 week"
            }
            button {
                class: "qchip",
                r#type: "button",
                disabled: value.is_none(),
                onclick: move |_| onpick.call(None),
                "Clear"
            }
        }
    }
}
