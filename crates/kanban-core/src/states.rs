//! State categories (`Proposed`/`InProgress`/`Resolved`/`Completed`/`Removed`) per work item type,
//! from `backlogconfiguration.workItemTypeMappedStates`. This is how the app reasons about custom
//! states without hardcoding their names.

use std::collections::HashMap;

use ado_client::BacklogConfigurationDto;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StateCategory {
    Proposed,
    InProgress,
    Resolved,
    Completed,
    Removed,
    Unknown,
}

impl StateCategory {
    pub fn parse(s: &str) -> Self {
        match s {
            "Proposed" => Self::Proposed,
            "InProgress" => Self::InProgress,
            "Resolved" => Self::Resolved,
            "Completed" => Self::Completed,
            "Removed" => Self::Removed,
            _ => Self::Unknown,
        }
    }

    /// Work is finished (counts towards progress, no overdue warning).
    pub fn is_done(self) -> bool {
        matches!(self, Self::Resolved | Self::Completed)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "Proposed",
            Self::InProgress => "InProgress",
            Self::Resolved => "Resolved",
            Self::Completed => "Completed",
            Self::Removed => "Removed",
            Self::Unknown => "Unknown",
        }
    }
}

/// Work item type → ordered states with their categories.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StateMap {
    types: HashMap<String, Vec<(String, StateCategory)>>,
}

impl StateMap {
    pub fn from_backlog(cfg: &BacklogConfigurationDto) -> Self {
        let order = |c: StateCategory| match c {
            StateCategory::Proposed => 0,
            StateCategory::InProgress => 1,
            StateCategory::Resolved => 2,
            StateCategory::Completed => 3,
            StateCategory::Removed => 4,
            StateCategory::Unknown => 5,
        };
        let types = cfg
            .work_item_type_mapped_states
            .iter()
            .map(|t| {
                let mut states: Vec<(String, StateCategory)> = t
                    .states
                    .iter()
                    .map(|(s, c)| (s.clone(), StateCategory::parse(c)))
                    .collect();
                // HashMap order is random: sort by category, then name, for stable pickers.
                states.sort_by(|a, b| order(a.1).cmp(&order(b.1)).then_with(|| a.0.cmp(&b.0)));
                (t.work_item_type_name.clone(), states)
            })
            .collect();
        Self { types }
    }

    pub fn category(&self, work_item_type: &str, state: &str) -> StateCategory {
        self.types
            .get(work_item_type)
            .and_then(|s| s.iter().find(|(name, _)| name == state))
            .map(|(_, c)| *c)
            .unwrap_or(StateCategory::Unknown)
    }

    /// States of a type, Proposed first.
    pub fn states(&self, work_item_type: &str) -> Vec<String> {
        self.types
            .get(work_item_type)
            .map(|s| s.iter().map(|(n, _)| n.clone()).collect())
            .unwrap_or_default()
    }

    /// Every state name in `category` across `types` (for WIQL filters).
    pub fn states_in(&self, types: &[String], category: StateCategory) -> Vec<String> {
        let mut out: Vec<String> = types
            .iter()
            .filter_map(|t| self.types.get(t))
            .flatten()
            .filter(|(_, c)| *c == category)
            .map(|(n, _)| n.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}
