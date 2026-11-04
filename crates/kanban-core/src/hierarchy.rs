//! Epic → Feature → Story tree for the epic list, built from `System.Parent`.
//!
//! Stories may hang directly off an epic or off one of its features; both appear as the epic's
//! children (features first in order, each followed by its stories). Tasks are not children:
//! they are kept per parent in `tasks`, so epic rows, progress and filters ignore them. Progress
//! counts children whose state category is Resolved or Completed.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::card::{Card, CardId};
use crate::states::StateMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Hierarchy {
    /// Root items of the list level (e.g. Epics), in backlog order.
    pub roots: Vec<CardId>,
    /// Root → its descendants (depth-first, ordered), with depth (1 = direct child).
    pub children: HashMap<CardId, Vec<(CardId, u8)>>,
    /// Any item (tasks included) → its root, if it has one.
    pub root_of: HashMap<CardId, CardId>,
    /// Parent → its tasks, in backlog order.
    pub tasks: HashMap<CardId, Vec<CardId>>,
}

impl Hierarchy {
    /// `root_types` are the work item types at the top of the list (from the highest visible
    /// portfolio backlog); `task_types` those kept in [`Hierarchy::tasks`].
    pub fn build(
        cards: &HashMap<CardId, Card>,
        root_types: &[String],
        task_types: &[String],
    ) -> Self {
        let mut kids: HashMap<CardId, Vec<&Card>> = HashMap::new();
        let mut tasks: HashMap<CardId, Vec<&Card>> = HashMap::new();
        for card in cards.values() {
            if let Some(p) = card.parent {
                let map = if task_types.contains(&card.work_item_type) {
                    &mut tasks
                } else {
                    &mut kids
                };
                map.entry(p).or_default().push(card);
            }
        }
        for list in kids.values_mut().chain(tasks.values_mut()) {
            list.sort_by(|a, b| order_cmp(a, b));
        }

        let mut roots: Vec<&Card> = cards
            .values()
            .filter(|c| root_types.contains(&c.work_item_type))
            .collect();
        roots.sort_by(|a, b| order_cmp(a, b));

        let mut out = Self {
            roots: roots.iter().map(|c| c.id).collect(),
            ..Self::default()
        };
        for root in roots {
            let mut list = Vec::new();
            let mut seen = HashSet::from([root.id]);
            let mut stack: Vec<(CardId, u8)> = kids
                .get(&root.id)
                .map(|v| v.iter().rev().map(|c| (c.id, 1)).collect())
                .unwrap_or_default();
            while let Some((id, depth)) = stack.pop() {
                if !seen.insert(id) {
                    continue; // parent cycles shouldn't exist, but never loop on bad data
                }
                list.push((id, depth));
                out.root_of.insert(id, root.id);
                if let Some(grand) = kids.get(&id) {
                    stack.extend(grand.iter().rev().map(|c| (c.id, depth.saturating_add(1))));
                }
            }
            out.children.insert(root.id, list);
        }
        for (parent, list) in tasks {
            if let Some(&root) = out
                .root_of
                .get(&parent)
                .or(out.roots.contains(&parent).then_some(&parent))
            {
                for t in &list {
                    out.root_of.insert(t.id, root);
                }
            }
            out.tasks
                .insert(parent, list.iter().map(|c| c.id).collect());
        }
        out
    }

    /// (done, total) over a root's descendants.
    pub fn progress(
        &self,
        root: CardId,
        cards: &HashMap<CardId, Card>,
        states: &StateMap,
    ) -> (usize, usize) {
        let Some(list) = self.children.get(&root) else {
            return (0, 0);
        };
        let total = list.len();
        let done = list
            .iter()
            .filter_map(|(id, _)| cards.get(id))
            .filter(|c| states.category(&c.work_item_type, &c.state).is_done())
            .count();
        (done, total)
    }
}

fn order_cmp(a: &Card, b: &Card) -> std::cmp::Ordering {
    match (a.order, b.order) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
    .then(a.id.cmp(&b.id))
}
