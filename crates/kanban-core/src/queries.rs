//! WIQL builders. Everything a query interpolates goes through [`quote`].

use ado_client::TeamFieldValuesDto;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::card::{Card, fields};

/// How many days Completed items stay visible (keeps the Closed column bounded).
pub const COMPLETED_WINDOW_DAYS: u32 = 30;

/// What a team "owns": its team field values (usually area paths) and the types we load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub project: String,
    pub team: String,
    /// Usually `System.AreaPath`.
    pub team_field: String,
    /// (value, include children)
    pub team_values: Vec<(String, bool)>,
    pub work_item_types: Vec<String>,
    /// States in the Completed category for those types.
    pub completed_states: Vec<String>,
    pub order_field: String,
}

impl Scope {
    pub fn team_values_from(dto: &TeamFieldValuesDto) -> Vec<(String, bool)> {
        dto.values
            .iter()
            .map(|v| (v.value.clone(), v.include_children))
            .collect()
    }

    /// The team clause, e.g. `([System.AreaPath] UNDER 'A' OR [System.AreaPath] = 'B')`.
    fn team_clause(&self) -> String {
        if self.team_values.is_empty() {
            return "1 = 1".into();
        }
        let parts: Vec<String> = self
            .team_values
            .iter()
            .map(|(v, children)| {
                let op = if *children { "UNDER" } else { "=" };
                format!("[{}] {op} {}", self.team_field, quote(v))
            })
            .collect();
        format!("({})", parts.join(" OR "))
    }

    fn types_clause(&self) -> String {
        format!(
            "[{}] IN ({})",
            fields::TYPE,
            self.work_item_types
                .iter()
                .map(|t| quote(t))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    /// Every card in scope; Completed ones only if changed recently.
    pub fn items_query(&self) -> String {
        let completed = if self.completed_states.is_empty() {
            String::new()
        } else {
            format!(
                " AND ([{state}] NOT IN ({list}) OR [{changed}] >= @Today - {COMPLETED_WINDOW_DAYS})",
                state = fields::STATE,
                list = self
                    .completed_states
                    .iter()
                    .map(|s| quote(s))
                    .collect::<Vec<_>>()
                    .join(", "),
                changed = fields::CHANGED_DATE,
            )
        };
        format!(
            "SELECT [{id}] FROM WorkItems WHERE [System.TeamProject] = @project AND {types} AND {team} AND [{state}] <> 'Removed'{completed} ORDER BY [{order}]",
            id = fields::ID,
            types = self.types_clause(),
            team = self.team_clause(),
            state = fields::STATE,
            order = self.order_field,
        )
    }

    /// Items of our types changed since `since`, regardless of area or state, so that
    /// items leaving the scope are seen too (see [`Scope::contains`]).
    pub fn delta_query(&self, since: OffsetDateTime) -> String {
        let since = since.format(&Rfc3339).unwrap_or_default();
        format!(
            "SELECT [{id}] FROM WorkItems WHERE [System.TeamProject] = @project AND {types} AND [{changed}] > {since} ORDER BY [{changed}]",
            id = fields::ID,
            types = self.types_clause(),
            changed = fields::CHANGED_DATE,
            since = quote(&since),
        )
    }

    /// Whether a (re)fetched card still belongs on screen.
    pub fn contains(&self, card: &Card) -> bool {
        if card.state == "Removed" || !self.work_item_types.contains(&card.work_item_type) {
            return false;
        }
        if self.team_field != fields::AREA_PATH || self.team_values.is_empty() {
            return true;
        }
        self.team_values.iter().any(|(v, children)| {
            card.area_path == *v
                || (*children
                    && card.area_path.len() > v.len()
                    && card.area_path.starts_with(v.as_str())
                    && card.area_path[v.len()..].starts_with('\\'))
        })
    }
}

/// A WIQL string literal.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope {
            project: "IT Finance".into(),
            team: "IT Finance Team".into(),
            team_field: "System.AreaPath".into(),
            team_values: vec![
                ("IT Finance".into(), true),
                ("Other\\O'Brien".into(), false),
            ],
            work_item_types: vec!["Epic".into(), "User Story".into()],
            completed_states: vec!["Closed".into()],
            order_field: "Microsoft.VSTS.Common.StackRank".into(),
        }
    }

    #[test]
    fn quotes_are_escaped() {
        assert_eq!(quote("O'Brien"), "'O''Brien'");
        assert!(
            scope()
                .items_query()
                .contains("[System.AreaPath] = 'Other\\O''Brien'")
        );
    }

    #[test]
    fn items_query_shape() {
        let q = scope().items_query();
        assert!(q.contains("[System.WorkItemType] IN ('Epic', 'User Story')"));
        assert!(q.contains("[System.AreaPath] UNDER 'IT Finance'"));
        assert!(q.contains("[System.State] <> 'Removed'"));
        assert!(q.contains("NOT IN ('Closed') OR [System.ChangedDate] >= @Today - 30"));
        assert!(q.ends_with("ORDER BY [Microsoft.VSTS.Common.StackRank]"));
    }

    #[test]
    fn delta_query_uses_changed_date() {
        let q = scope().delta_query(OffsetDateTime::UNIX_EPOCH);
        assert!(q.contains("[System.ChangedDate] > '1970-01-01T00:00:00Z'"));
        assert!(!q.contains("AreaPath"));
    }
}
