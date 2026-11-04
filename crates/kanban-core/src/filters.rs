//! Filters of the board screen: the shared iteration, and each tab's own assignees (`@me`,
//! `@unassigned` or a `unique_name`), area paths, epic states and work item states, as saved in
//! [`TabFilters`] and [`crate::BoardPrefs`]. Also which states the kanban shows, for the status
//! filter's options.

use crate::board::Board;
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

impl TabFilters {
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

/// Work items (not roots) for the timeline. With an epic state filter, items without an epic
/// are left out.
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
        .filter(|c| prefs.matches_item(c, iteration, me))
        .filter(|c| {
            prefs.epic_states.is_empty()
                || hierarchy
                    .root_of
                    .get(&c.id)
                    .and_then(|r| ws.cards.get(r))
                    .is_some_and(|e| matches_states(&e.state, &prefs.epic_states))
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
            }),
            iteration_path: iteration.into(),
            state: "Active".into(),
            ..Card::default()
        }
    }

    #[test]
    fn assignee_tokens() {
        let me = Person {
            display_name: "Me".into(),
            unique_name: "me@x".into(),
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
}
