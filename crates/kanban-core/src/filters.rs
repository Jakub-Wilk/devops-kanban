//! Filters of the board screen: the shared iteration, and each tab's own assignees (`@me`,
//! `@unassigned` or a `unique_name`), area paths, epic statuses and work item statuses, as saved in
//! [`TabFilters`] and [`crate::BoardPrefs`]. Also which states the kanban shows, for the status
//! filter's options. And the List's tree: every level from the root down to tasks, with the
//! ancestors of matching items kept as context.

use crate::board::Board;
use std::collections::{HashMap, HashSet};

use crate::card::{Card, CardId, Person};
use crate::config::{BoardPrefs, Settings, TabFilters};
use crate::hierarchy::Hierarchy;
use crate::workspace::Workspace;

/// Assignee token for the signed-in user.
pub const ME: &str = "@me";
/// Assignee token for unassigned items.
pub const UNASSIGNED: &str = "@unassigned";

/// `path` is `parent` or below it (`A\\B` is under `A`, `AB` is not).
fn under(path: &str, parent: &str) -> bool {
    path == parent || (path.starts_with(parent) && path[parent.len()..].starts_with('\\'))
}

/// Is the card in `iteration` or one of its children? `None` matches everything.
pub fn in_iteration(card: &Card, iteration: Option<&str>) -> bool {
    iteration.is_none_or(|it| under(&card.iteration_path, it))
}

/// Is the card in one of `areas` or below it? Empty matches everything.
pub fn matches_areas(card: &Card, areas: &[String]) -> bool {
    areas.is_empty() || areas.iter().any(|a| under(&card.area_path, a))
}

/// Kanban card passes the board's assignee and area filters.
pub fn matches_board(card: &Card, prefs: &BoardPrefs, me: Option<&Person>) -> bool {
    matches_assignees(card, &prefs.assignees, me) && matches_areas(card, &prefs.areas)
}

pub fn matches_assignee(card: &Card, token: &str, me: Option<&Person>) -> bool {
    let unique = card.assigned_to.as_ref().map(|a| a.unique_name.as_str());
    match token {
        ME => me.is_some_and(|m| unique == Some(m.unique_name.as_str())),
        UNASSIGNED => unique.is_none(),
        u => unique == Some(u),
    }
}

/// Matches any of `tokens`; empty matches everyone.
pub fn matches_assignees(card: &Card, tokens: &[String], me: Option<&Person>) -> bool {
    tokens.is_empty() || tokens.iter().any(|t| matches_assignee(card, t, me))
}

/// Empty matches every state.
pub fn matches_states(state: &str, states: &[String]) -> bool {
    states.is_empty() || states.iter().any(|s| s == state)
}

/// `items`, or none while `off`.
fn unless_off<T: Clone>(items: &[T], off: bool) -> Vec<T> {
    if off { Vec::new() } else { items.to_vec() }
}

impl BoardPrefs {
    /// The preferences to apply: the filters switched off cleared (and no longer marked off).
    pub fn applied(&self) -> BoardPrefs {
        BoardPrefs {
            epic: self.epic.filter(|_| !self.epic_off),
            assignees: unless_off(&self.assignees, self.assignees_off),
            areas: unless_off(&self.areas, self.areas_off),
            epic_off: false,
            assignees_off: false,
            areas_off: false,
            ..self.clone()
        }
    }
}

impl TabFilters {
    /// The filters to apply: those switched off cleared (and no longer marked off).
    pub fn applied(&self) -> TabFilters {
        TabFilters {
            epic_states: unless_off(&self.epic_states, self.epic_states_off),
            assignees: unless_off(&self.assignees, self.assignees_off),
            areas: unless_off(&self.areas, self.areas_off),
            item_states: unless_off(&self.item_states, self.item_states_off),
            ..TabFilters::default()
        }
    }

    /// An assignee, area or work item state filter is set (iteration and epic states aside).
    pub fn filters_items(&self) -> bool {
        !self.assignees.is_empty() || !self.areas.is_empty() || !self.item_states.is_empty()
    }

    /// Any filter is set.
    pub fn is_filtering(&self) -> bool {
        self.filters_items() || !self.epic_states.is_empty()
    }

    /// Work item passes the iteration, assignee, area and state filters.
    pub fn matches_item(&self, card: &Card, iteration: Option<&str>, me: Option<&Person>) -> bool {
        in_iteration(card, iteration)
            && matches_assignees(card, &self.assignees, me)
            && matches_areas(card, &self.areas)
            && matches_states(&card.state, &self.item_states)
    }
}

/// The epic list: roots in backlog order with their matching descendants. An epic with no
/// matching descendant still shows when it matches the assignee and area filters itself (and no
/// work item state is selected). `text` is the search box, matched against the epic or any of its
/// shown descendants.
pub fn epic_rows<'a>(
    ws: &'a Workspace,
    hierarchy: &Hierarchy,
    prefs: &TabFilters,
    iteration: Option<&str>,
    text: impl Fn(&Card) -> bool,
) -> Vec<(&'a Card, Vec<(&'a Card, u8)>)> {
    let me = ws.me.as_ref();
    hierarchy
        .roots
        .iter()
        .filter_map(|id| ws.cards.get(id))
        .filter(|epic| matches_states(&epic.state, &prefs.epic_states))
        .filter_map(|epic| {
            let kids: Vec<(&Card, u8)> = hierarchy
                .children
                .get(&epic.id)
                .into_iter()
                .flatten()
                .filter_map(|(id, depth)| ws.cards.get(id).map(|c| (c, *depth)))
                .filter(|(c, _)| prefs.matches_item(c, iteration, me))
                .collect();
            let items_ok = !prefs.filters_items()
                || !kids.is_empty()
                || (prefs.item_states.is_empty()
                    && matches_assignees(epic, &prefs.assignees, me)
                    && matches_areas(epic, &prefs.areas)
                    && in_iteration(epic, iteration));
            let iter_ok = iteration.is_none() || in_iteration(epic, iteration) || !kids.is_empty();
            let text_ok = text(epic) || kids.iter().any(|(c, _)| text(c));
            (items_ok && iter_ok && text_ok).then_some((epic, kids))
        })
        .collect()
}

/// Backlog items without a parent ([`Hierarchy::orphans`]), each with its matching descendants,
/// for the List's "without a parent" group. An orphan shows when it or something below it
/// matches the work item filters and the search. The epic status filter doesn't apply: they
/// have no epic.
pub fn orphan_rows(
    ws: &Workspace,
    hierarchy: &Hierarchy,
    prefs: &TabFilters,
    iteration: Option<&str>,
    text: impl Fn(&Card) -> bool,
) -> Vec<(CardId, Vec<CardId>)> {
    let me = ws.me.as_ref();
    hierarchy
        .orphans
        .iter()
        .filter_map(|id| ws.cards.get(id))
        .filter_map(|top| {
            let kids: Vec<&Card> = hierarchy
                .children
                .get(&top.id)
                .into_iter()
                .flatten()
                .filter_map(|(id, _)| ws.cards.get(id))
                .filter(|c| prefs.matches_item(c, iteration, me))
                .collect();
            let itself = prefs.matches_item(top, iteration, me);
            let text_ok = text(top) || kids.iter().any(|c| text(c));
            ((itself || !kids.is_empty()) && text_ok)
                .then(|| (top.id, kids.iter().map(|c| c.id).collect()))
        })
        .collect()
}

/// The one person every card is assigned to, if there is exactly one. `None` when there are no
/// cards, any card is unassigned, or two people appear. The kanban's WIP meter only makes sense
/// for one person's cards.
pub fn single_assignee<'a>(cards: impl IntoIterator<Item = &'a Card>) -> Option<String> {
    let mut who: Option<&str> = None;
    for c in cards {
        let u = c.assigned_to.as_ref()?.unique_name.as_str();
        match who {
            None => who = Some(u),
            Some(w) if w != u => return None,
            Some(_) => {}
        }
    }
    who.map(str::to_owned)
}

/// The kanban is filtered to exactly one person (a single assignee token other than
/// `@unassigned`), so the WIP meter applies even to a column with none of their cards.
pub fn filtered_to_one_person(prefs: &BoardPrefs) -> bool {
    matches!(prefs.assignees.as_slice(), [t] if t != UNASSIGNED)
}

/// One row of the List's tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListRow {
    pub id: CardId,
    /// Indentation: 0 for roots, +1 per parent.
    pub depth: u8,
    pub parent: Option<CardId>,
    /// Index of the row's backlog level in [`list_levels`] (0 = roots). Drives "Expand down to".
    pub rank: usize,
    /// The next row is one of its children (decides the twirl).
    pub has_children: bool,
    /// Shown only because something below it matches the filters.
    pub context: bool,
    pub task: bool,
    /// In the "without a parent" group (see [`Hierarchy::orphans`]), after the roots.
    pub orphan: bool,
}

/// The List's levels, top down: the root portfolio level, the levels below it, then tasks.
/// Each is (name, work item types). Names come from the team's backlog configuration.
pub fn list_levels(ws: &Workspace) -> Vec<(String, Vec<String>)> {
    let start = ws.levels.iter().position(|l| l.is_portfolio).unwrap_or(0);
    let mut out: Vec<(String, Vec<String>)> = ws.levels[start.min(ws.levels.len())..]
        .iter()
        .map(|l| (l.name.clone(), l.work_item_types.clone()))
        .collect();
    if !ws.task_types.is_empty() {
        out.push(("Tasks".into(), ws.task_types.clone()));
    }
    out
}

/// The List as a flat tree: each epic from [`epic_rows`], then its descendants depth-first, each
/// followed by its tasks; then the items without a parent ([`orphan_rows`], `orphan` set) the
/// same way (the epic status filter only narrows the epics). Descendants that don't match the filters still show (as `context`) when something
/// below them does.
pub fn list_rows(
    ws: &Workspace,
    hierarchy: &Hierarchy,
    prefs: &TabFilters,
    iteration: Option<&str>,
    text: impl Fn(&Card) -> bool,
) -> Vec<ListRow> {
    let matched: Vec<(CardId, Vec<CardId>)> = epic_rows(ws, hierarchy, prefs, iteration, &text)
        .into_iter()
        .map(|(e, kids)| (e.id, kids.into_iter().map(|(c, _)| c.id).collect()))
        .collect();
    let levels = list_levels(ws);
    let rank = |c: &Card| {
        levels
            .iter()
            .position(|(_, types)| types.contains(&c.work_item_type))
            .unwrap_or(levels.len().saturating_sub(1))
    };
    let mut rows = tree_rows(&ws.cards, hierarchy, &matched, &ws.task_types, rank);
    let orphans = orphan_rows(ws, hierarchy, prefs, iteration, &text);
    let mut tail = tree_rows(&ws.cards, hierarchy, &orphans, &ws.task_types, rank);
    for r in &mut tail {
        r.orphan = true;
    }
    rows.extend(tail);
    rows
}

/// [`list_rows`] without the filtering: `matched` is each root with its matching descendants.
pub fn tree_rows(
    cards: &HashMap<CardId, Card>,
    hierarchy: &Hierarchy,
    matched: &[(CardId, Vec<CardId>)],
    task_types: &[String],
    rank: impl Fn(&Card) -> usize,
) -> Vec<ListRow> {
    let mut out: Vec<ListRow> = Vec::new();
    let push_tasks = |out: &mut Vec<ListRow>, parent: CardId, depth: u8| {
        for t in hierarchy.tasks.get(&parent).into_iter().flatten() {
            if let Some(c) = cards.get(t) {
                out.push(ListRow {
                    id: *t,
                    depth: depth.saturating_add(1),
                    parent: Some(parent),
                    rank: rank(c),
                    has_children: false,
                    context: false,
                    task: true,
                    orphan: false,
                });
            }
        }
    };
    for (root, kids) in matched {
        let Some(root_card) = cards.get(root) else {
            continue;
        };
        let hits: HashSet<CardId> = kids.iter().copied().collect();
        // Every ancestor of a hit, up to the root, stays as context.
        let mut keep = hits.clone();
        for id in kids {
            let mut p = cards.get(id).and_then(|c| c.parent);
            while let Some(pid) = p {
                if pid == *root || !keep.insert(pid) {
                    break;
                }
                p = cards.get(&pid).and_then(|c| c.parent);
            }
        }
        out.push(ListRow {
            id: *root,
            depth: 0,
            parent: None,
            rank: rank(root_card),
            has_children: false,
            context: false,
            task: false,
            orphan: false,
        });
        push_tasks(&mut out, *root, 0);
        for (id, depth) in hierarchy.children.get(root).into_iter().flatten() {
            if !keep.contains(id) {
                continue;
            }
            let Some(c) = cards.get(id) else { continue };
            let context = !hits.contains(id);
            out.push(ListRow {
                id: *id,
                depth: *depth,
                parent: c.parent,
                rank: rank(c),
                has_children: false,
                context,
                task: task_types.contains(&c.work_item_type),
                orphan: false,
            });
            if !context {
                push_tasks(&mut out, *id, *depth);
            }
        }
    }
    for i in 0..out.len().saturating_sub(1) {
        out[i].has_children = out[i + 1].depth > out[i].depth;
    }
    out
}

/// Re-orders `rows` within each parent by `key` (stable, so ties keep backlog order), keeping
/// every subtree under its row.
pub fn sort_siblings<K: Ord>(rows: Vec<ListRow>, key: impl Fn(&ListRow) -> K) -> Vec<ListRow> {
    // Split a run of rows into subtrees that start at `depth`.
    fn split(rows: &[ListRow]) -> Vec<&[ListRow]> {
        let mut out = Vec::new();
        let mut start = 0;
        for i in 1..=rows.len() {
            if i == rows.len() || rows[i].depth <= rows[start].depth {
                out.push(&rows[start..i]);
                start = i;
            }
        }
        out
    }
    fn sort<K: Ord>(rows: &[ListRow], key: &impl Fn(&ListRow) -> K, out: &mut Vec<ListRow>) {
        let mut trees = split(rows);
        trees.sort_by_key(|t| key(&t[0]));
        for t in trees {
            out.push(t[0]);
            sort(&t[1..], key, out);
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    sort(&rows, &key, &mut out);
    out
}

/// Work items (not roots) for the timeline. The epic status filter applies to items in an epic;
/// items without one stay in. Tasks whose parent is loaded are left out too: they follow their parent (see
/// [`Hierarchy::tasks`]) rather than being filtered themselves.
pub fn timeline_items<'a>(
    ws: &'a Workspace,
    hierarchy: &Hierarchy,
    prefs: &TabFilters,
    iteration: Option<&str>,
) -> Vec<&'a Card> {
    let roots = ws.root_types();
    let me = ws.me.as_ref();
    ws.cards
        .values()
        .filter(|c| !roots.contains(&c.work_item_type))
        .filter(|c| !ws.is_task(c) || c.parent.is_none_or(|p| !ws.cards.contains_key(&p)))
        .filter(|c| prefs.matches_item(c, iteration, me))
        .filter(|c| {
            prefs.epic_states.is_empty()
                || hierarchy
                    .root_of
                    .get(&c.id)
                    .and_then(|r| ws.cards.get(r))
                    .is_none_or(|e| matches_states(&e.state, &prefs.epic_states))
        })
        .collect()
}

/// Cards on `board` in `epic` (when set) and the iteration, before the assignee filter.
pub fn kanban_cards<'a>(
    ws: &'a Workspace,
    hierarchy: &Hierarchy,
    board: &Board,
    epic: Option<CardId>,
    iteration: Option<&str>,
) -> Vec<&'a Card> {
    ws.cards
        .values()
        .filter(|c| board.shows(c))
        .filter(|c| epic.is_none_or(|e| hierarchy.root_of.get(&c.id) == Some(&e)))
        .filter(|c| in_iteration(c, iteration))
        .collect()
}

/// States an item of `work_item_type` may be set to: those its kanban's visible columns map, each
/// with that column (see [`Board::visible_states`]). `None` when the type has no kanban (tasks,
/// the top level), where every state of the type is allowed.
pub fn board_states(
    ws: &Workspace,
    settings: &Settings,
    work_item_type: &str,
) -> Option<Vec<(String, String)>> {
    let level = ws.level_of_type(work_item_type)?;
    if !ws.kanban_levels().iter().any(|l| l.name == level.name) {
        return None;
    }
    let board = ws.boards.get(&level.name)?;
    let hidden = settings
        .board_prefs(&ws.project, &ws.team, &level.name)
        .hidden_columns;
    Some(board.visible_states(work_item_type, &hidden))
}

/// States mapped by the kanban columns that aren't hidden, in column order, across the kanban
/// levels' boards.
pub fn kanban_states(ws: &Workspace, settings: &Settings) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for level in ws.kanban_levels() {
        let Some(board) = ws.boards.get(&level.name) else {
            continue;
        };
        let hidden = settings
            .board_prefs(&ws.project, &ws.team, &level.name)
            .hidden_columns;
        for column in board.visible_columns(&hidden) {
            let mut states: Vec<&String> = column.state_mappings.values().collect();
            states.sort();
            for s in states {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(assignee: Option<&str>, iteration: &str) -> Card {
        Card {
            assigned_to: assignee.map(|u| Person {
                display_name: u.into(),
                unique_name: u.into(),
                ..Person::default()
            }),
            iteration_path: iteration.into(),
            state: "Active".into(),
            ..Card::default()
        }
    }

    fn node(id: CardId, parent: Option<CardId>, ty: &str, order: f64) -> (CardId, Card) {
        (
            id,
            Card {
                id,
                parent,
                work_item_type: ty.into(),
                order: Some(order),
                ..Card::default()
            },
        )
    }

    /// Epic 1 › Feature 2 › Story 3 › Task 5, and Story 4 directly under the epic.
    fn tree() -> (HashMap<CardId, Card>, Hierarchy) {
        let cards: HashMap<CardId, Card> = [
            node(1, None, "Epic", 1.0),
            node(2, Some(1), "Feature", 1.0),
            node(3, Some(2), "User Story", 1.0),
            node(4, Some(1), "User Story", 2.0),
            node(5, Some(3), "Task", 1.0),
        ]
        .into_iter()
        .collect();
        let h = Hierarchy::build(&cards, &["Epic".into()], &["Task".into()]);
        (cards, h)
    }

    fn rank(c: &Card) -> usize {
        ["Epic", "Feature", "User Story", "Task"]
            .iter()
            .position(|t| *t == c.work_item_type)
            .unwrap_or(3)
    }

    /// A workspace with Epics as the root level and Tasks as the task backlog.
    fn workspace(cards: HashMap<CardId, Card>) -> Workspace {
        let mut ws: Workspace = serde_json::from_value(serde_json::json!({
            "project": "P", "team": "T",
            "levels": [{ "name": "Epics", "rank": 0, "work_item_types": ["Epic"],
                         "default_type": "Epic", "is_portfolio": true }],
            "task_types": ["Task"],
            "states": { "types": {} },
            "scope": { "project": "P", "team": "T", "team_field": "System.AreaPath",
                       "team_values": [], "work_item_types": [], "completed_states": [],
                       "order_field": "Microsoft.VSTS.Common.StackRank" },
            "fields": [], "cards": {}, "boards": {}, "board_errors": {},
            "iterations": [], "areas": [], "me": null, "synced_to": null
        }))
        .unwrap();
        ws.cards = cards;
        ws
    }

    /// [`tree`] with Epic 1 `Active`, plus a parentless story 21 and Epic 40 `New` with story 41.
    fn filter_fixture() -> (Workspace, Hierarchy) {
        let (mut cards, _) = tree();
        cards.extend([
            node(21, None, "User Story", 1.0),
            node(40, None, "Epic", 2.0),
            node(41, Some(40), "User Story", 1.0),
        ]);
        for c in cards.values_mut() {
            c.state = "Active".into();
        }
        if let Some(e) = cards.get_mut(&40) {
            e.state = "New".into();
        }
        let h = Hierarchy::build(&cards, &["Epic".into()], &["Task".into()]);
        (workspace(cards), h)
    }

    #[test]
    fn epic_status_filter_keeps_items_without_a_parent() {
        let (ws, h) = filter_fixture();
        let prefs = TabFilters {
            epic_states: vec!["Active".into()],
            ..TabFilters::default()
        };
        let epics: Vec<CardId> = epic_rows(&ws, &h, &prefs, None, |_| true)
            .iter()
            .map(|(e, _)| e.id)
            .collect();
        assert_eq!(epics, [1], "the New epic is filtered out");
        let orphans: Vec<CardId> = orphan_rows(&ws, &h, &prefs, None, |_| true)
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(orphans, [21], "parentless items stay");
        let rows: Vec<CardId> = list_rows(&ws, &h, &prefs, None, |_| true)
            .iter()
            .map(|r| r.id)
            .collect();
        assert!(rows.contains(&21));
        assert!(!rows.contains(&40) && !rows.contains(&41), "{rows:?}");
    }

    #[test]
    fn timeline_epic_status_filter_keeps_items_without_an_epic() {
        let (ws, h) = filter_fixture();
        let prefs = TabFilters {
            epic_states: vec!["Active".into()],
            ..TabFilters::default()
        };
        let mut ids: Vec<CardId> = timeline_items(&ws, &h, &prefs, None)
            .iter()
            .map(|c| c.id)
            .collect();
        ids.sort_unstable();
        // Task 5's parent is loaded, so it follows story 3 rather than being listed.
        assert_eq!(ids, [2, 3, 4, 21]);
    }

    #[test]
    fn parentless_items_are_orphans_with_their_own_trees() {
        let (mut cards, _) = tree();
        cards.extend([
            // One-off feature with a story and a task below it.
            node(10, None, "Feature", 3.0),
            node(11, Some(10), "User Story", 1.0),
            node(12, Some(11), "Task", 1.0),
            // A story whose parent isn't loaded, and a parentless one.
            node(20, Some(999), "User Story", 2.0),
            node(21, None, "User Story", 1.0),
            // A parentless task is not a backlog item: it stays out.
            node(30, None, "Task", 1.0),
        ]);
        let h = Hierarchy::build(&cards, &["Epic".into()], &["Task".into()]);
        assert_eq!(h.roots, [1]);
        assert_eq!(h.orphans, [21, 20, 10], "backlog order");
        assert_eq!(h.children[&10], [(11, 1)]);
        assert_eq!(h.tasks[&11], [12]);
        // Orphan trees have no epic.
        assert!(!h.root_of.contains_key(&11));

        let rows = tree_rows(&cards, &h, &[(10, vec![11])], &["Task".into()], rank);
        let got: Vec<(CardId, u8, bool)> = rows
            .iter()
            .map(|r| (r.id, r.depth, r.has_children))
            .collect();
        assert_eq!(got, [(10, 0, true), (11, 1, true), (12, 2, false)]);
    }

    #[test]
    fn tree_rows_order_depth_and_children() {
        let (cards, h) = tree();
        let rows = tree_rows(&cards, &h, &[(1, vec![2, 3, 4])], &["Task".into()], rank);
        let got: Vec<(CardId, u8, usize, bool, bool)> = rows
            .iter()
            .map(|r| (r.id, r.depth, r.rank, r.has_children, r.task))
            .collect();
        assert_eq!(
            got,
            vec![
                (1, 0, 0, true, false),
                (2, 1, 1, true, false),
                (3, 2, 2, true, false),
                (5, 3, 3, false, true),
                (4, 1, 2, false, false),
            ]
        );
        assert_eq!(rows[3].parent, Some(3));
        assert!(rows.iter().all(|r| !r.context));
    }

    #[test]
    fn tree_rows_keep_ancestors_as_context() {
        let (cards, h) = tree();
        // Only the deep story matches: its feature stays as context, the other story goes.
        let rows = tree_rows(&cards, &h, &[(1, vec![3])], &["Task".into()], rank);
        let got: Vec<(CardId, bool)> = rows.iter().map(|r| (r.id, r.context)).collect();
        assert_eq!(got, vec![(1, false), (2, true), (3, false), (5, false)]);
    }

    #[test]
    fn sort_siblings_keeps_subtrees() {
        let (cards, h) = tree();
        let rows = tree_rows(&cards, &h, &[(1, vec![2, 3, 4])], &["Task".into()], rank);
        // Reverse by id: story 4 moves above feature 2, which keeps story 3 and task 5 below it.
        let sorted = sort_siblings(rows, |r| std::cmp::Reverse(r.id));
        let ids: Vec<CardId> = sorted.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![1, 4, 2, 3, 5]);
    }

    #[test]
    fn single_assignee_needs_exactly_one_person() {
        let a = card(Some("a@x"), "P");
        let a2 = card(Some("a@x"), "P");
        let b = card(Some("b@x"), "P");
        let nobody = card(None, "P");
        assert_eq!(single_assignee([&a, &a2]), Some("a@x".into()));
        assert_eq!(single_assignee([&a, &b]), None);
        assert_eq!(single_assignee([&a, &nobody]), None);
        assert_eq!(single_assignee([&nobody]), None);
        assert_eq!(single_assignee(std::iter::empty::<&Card>()), None);
    }

    #[test]
    fn one_person_filter() {
        let prefs = |t: &[&str]| BoardPrefs {
            assignees: t.iter().map(|s| s.to_string()).collect(),
            ..BoardPrefs::default()
        };
        assert!(filtered_to_one_person(&prefs(&[ME])));
        assert!(filtered_to_one_person(&prefs(&["a@x"])));
        assert!(!filtered_to_one_person(&prefs(&[UNASSIGNED])));
        assert!(!filtered_to_one_person(&prefs(&[ME, "a@x"])));
        assert!(!filtered_to_one_person(&prefs(&[])));
    }

    #[test]
    fn assignee_tokens() {
        let me = Person {
            display_name: "Me".into(),
            unique_name: "me@x".into(),
            ..Person::default()
        };
        let mine = card(Some("me@x"), "P");
        let theirs = card(Some("os@x"), "P");
        let nobody = card(None, "P");
        assert!(matches_assignee(&mine, ME, Some(&me)));
        assert!(!matches_assignee(&theirs, ME, Some(&me)));
        assert!(!matches_assignee(&mine, ME, None));
        assert!(matches_assignee(&nobody, UNASSIGNED, Some(&me)));
        assert!(matches_assignee(&theirs, "os@x", Some(&me)));
        let tokens = vec![ME.to_owned(), UNASSIGNED.to_owned()];
        assert!(matches_assignees(&nobody, &tokens, Some(&me)));
        assert!(!matches_assignees(&theirs, &tokens, Some(&me)));
        assert!(matches_assignees(&theirs, &[], Some(&me)));
    }

    #[test]
    fn iteration_includes_children_only() {
        let c = card(None, "P\\Sprint 1\\Week 2");
        assert!(in_iteration(&c, None));
        assert!(in_iteration(&c, Some("P\\Sprint 1")));
        assert!(!in_iteration(&c, Some("P\\Sprint 1\\Week")));
        assert!(!in_iteration(&c, Some("P\\Sprint 2")));
    }

    #[test]
    fn areas_include_sub_areas() {
        let mut c = card(None, "P");
        c.area_path = "IT Finance\\Rozwój\\KSeF".into();
        assert!(matches_areas(&c, &[]));
        assert!(matches_areas(&c, &["IT Finance\\Rozwój".into()]));
        assert!(!matches_areas(&c, &["IT Finance\\Roz".into()]));
        assert!(matches_areas(
            &c,
            &["IT Finance\\Penny".into(), "IT Finance".into()]
        ));
        let prefs = TabFilters {
            areas: vec!["IT Finance\\Penny".into()],
            ..TabFilters::default()
        };
        assert!(prefs.filters_items());
        assert!(!prefs.matches_item(&c, None, None));
    }

    #[test]
    fn item_filters() {
        let prefs = TabFilters {
            item_states: vec!["Blocked".into()],
            ..TabFilters::default()
        };
        assert!(prefs.filters_items());
        assert!(!prefs.matches_item(&card(None, "P"), None, None));
        assert!(TabFilters::default().matches_item(&card(None, "P\\S1"), Some("P\\S1"), None));
        assert!(!TabFilters::default().matches_item(&card(None, "P\\S1"), Some("P\\S2"), None));
        assert!(matches_states("Active", &[]));
        assert!(!TabFilters::default().is_filtering());
    }

    #[test]
    fn filters_switched_off_keep_their_selection() {
        let prefs = TabFilters {
            assignees: vec![ME.into()],
            areas: vec!["P\\A".into()],
            assignees_off: true,
            ..TabFilters::default()
        };
        assert!(prefs.is_filtering());
        let applied = prefs.applied();
        assert!(applied.assignees.is_empty());
        assert_eq!(applied.areas, prefs.areas);
        assert!(
            !TabFilters {
                areas_off: true,
                ..applied
            }
            .applied()
            .is_filtering()
        );

        let board = BoardPrefs {
            epic: Some(1),
            assignees: vec![ME.into()],
            areas: vec!["P\\A".into()],
            hidden_columns: vec!["Closed".into()],
            epic_off: true,
            areas_off: true,
            ..BoardPrefs::default()
        };
        let applied = board.applied();
        assert_eq!(applied.epic, None);
        assert_eq!(applied.assignees, board.assignees);
        assert!(applied.areas.is_empty());
        assert_eq!(applied.hidden_columns, board.hidden_columns);
    }
}
