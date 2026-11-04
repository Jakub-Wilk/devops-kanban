//! Card = the subset of a work item a board needs, plus its revision for concurrency.
//!
//! The field list to fetch comes from the board's `cardsettings` plus the fixed set the domain
//! needs (id, rev, type, state, board column/lane, order, parent, scheduling).

use std::collections::BTreeMap;

use ado_client::{IdentityRef, WorkItemDto};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, OffsetDateTime};

pub type CardId = u32;

/// Reference names of the fields the domain reads.
pub mod fields {
    pub const ID: &str = "System.Id";
    pub const REV: &str = "System.Rev";
    pub const TYPE: &str = "System.WorkItemType";
    pub const TITLE: &str = "System.Title";
    pub const STATE: &str = "System.State";
    pub const BOARD_COLUMN: &str = "System.BoardColumn";
    pub const BOARD_COLUMN_DONE: &str = "System.BoardColumnDone";
    pub const BOARD_LANE: &str = "System.BoardLane";
    pub const ASSIGNED_TO: &str = "System.AssignedTo";
    pub const TAGS: &str = "System.Tags";
    pub const PARENT: &str = "System.Parent";
    pub const AREA_PATH: &str = "System.AreaPath";
    pub const ITERATION_PATH: &str = "System.IterationPath";
    pub const CHANGED_DATE: &str = "System.ChangedDate";
    pub const PRIORITY: &str = "Microsoft.VSTS.Common.Priority";
    pub const TARGET_DATE: &str = "Microsoft.VSTS.Scheduling.TargetDate";
    pub const COMPLETED_WORK: &str = "Microsoft.VSTS.Scheduling.CompletedWork";
}

/// Someone a card can be assigned to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Person {
    pub display_name: String,
    /// What `System.AssignedTo` accepts on write (usually the e-mail).
    pub unique_name: String,
}

impl Person {
    pub fn initials(&self) -> String {
        let mut words = self
            .display_name
            .split(|c: char| c.is_whitespace() || c == '-' || c == '.')
            .filter(|w| !w.is_empty());
        let first = words.next().and_then(|w| w.chars().next());
        let last = words.next_back().and_then(|w| w.chars().next());
        first
            .into_iter()
            .chain(last)
            .flat_map(char::to_uppercase)
            .collect()
    }
}

impl From<&IdentityRef> for Person {
    fn from(i: &IdentityRef) -> Self {
        Self {
            display_name: i.display_name.clone(),
            unique_name: i
                .unique_name
                .clone()
                .unwrap_or_else(|| i.display_name.clone()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub rev: u32,
    pub work_item_type: String,
    pub title: String,
    pub state: String,
    pub board_column: Option<String>,
    pub board_column_done: bool,
    pub board_lane: Option<String>,
    pub assigned_to: Option<Person>,
    pub tags: Vec<String>,
    pub order: Option<f64>,
    pub parent: Option<CardId>,
    /// 1 (critical) – 4 (low).
    pub priority: Option<u8>,
    pub target_date: Option<Date>,
    pub area_path: String,
    pub iteration_path: String,
    /// Hours, kept in sync by 7pace.
    pub completed_work: Option<f64>,
    pub changed_date: Option<OffsetDateTime>,
    /// Remaining card-setting fields, raw.
    pub extra: BTreeMap<String, Value>,
}

impl Card {
    /// Builds a card from a hydrated work item. `order_field` is the backlog's Order field.
    pub fn from_work_item(dto: &WorkItemDto, order_field: &str) -> Self {
        let f = &dto.fields;
        let str_of = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_owned);
        const KNOWN: &[&str] = &[
            fields::ID,
            fields::REV,
            fields::TYPE,
            fields::TITLE,
            fields::STATE,
            fields::BOARD_COLUMN,
            fields::BOARD_COLUMN_DONE,
            fields::BOARD_LANE,
            fields::ASSIGNED_TO,
            fields::TAGS,
            fields::PARENT,
            fields::AREA_PATH,
            fields::ITERATION_PATH,
            fields::CHANGED_DATE,
            fields::PRIORITY,
            fields::TARGET_DATE,
            fields::COMPLETED_WORK,
        ];
        Self {
            id: dto.id,
            rev: dto.rev,
            work_item_type: str_of(fields::TYPE).unwrap_or_default(),
            title: str_of(fields::TITLE).unwrap_or_default(),
            state: str_of(fields::STATE).unwrap_or_default(),
            board_column: str_of(fields::BOARD_COLUMN),
            board_column_done: f
                .get(fields::BOARD_COLUMN_DONE)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            board_lane: str_of(fields::BOARD_LANE),
            assigned_to: f
                .get(fields::ASSIGNED_TO)
                .and_then(|v| serde_json::from_value::<IdentityRef>(v.clone()).ok())
                .map(|i| Person::from(&i)),
            tags: str_of(fields::TAGS)
                .map(|t| {
                    t.split(';')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            order: f.get(order_field).and_then(Value::as_f64),
            parent: f
                .get(fields::PARENT)
                .and_then(Value::as_u64)
                .map(|p| p as CardId),
            priority: f
                .get(fields::PRIORITY)
                .and_then(Value::as_u64)
                .map(|p| p.clamp(1, 4) as u8),
            target_date: str_of(fields::TARGET_DATE)
                .as_deref()
                .and_then(parse_date_field),
            area_path: str_of(fields::AREA_PATH).unwrap_or_default(),
            iteration_path: str_of(fields::ITERATION_PATH).unwrap_or_default(),
            completed_work: f.get(fields::COMPLETED_WORK).and_then(Value::as_f64),
            changed_date: str_of(fields::CHANGED_DATE)
                .and_then(|s| OffsetDateTime::parse(&s, &Rfc3339).ok()),
            extra: f
                .iter()
                .filter(|(k, _)| !KNOWN.contains(&k.as_str()) && k.as_str() != order_field)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }

    /// Last segment of the area path (`IT Finance\Penny` → `Penny`).
    pub fn area_leaf(&self) -> &str {
        self.area_path
            .rsplit('\\')
            .next()
            .unwrap_or(&self.area_path)
    }

    /// Last segment of the iteration path.
    pub fn iteration_leaf(&self) -> &str {
        self.iteration_path
            .rsplit('\\')
            .next()
            .unwrap_or(&self.iteration_path)
    }
}

/// Date fields are stored as a UTC instant; the web UI writes local midnight, which can land on
/// the previous UTC day. Rounding to the nearest day recovers the intended date for any offset
/// within ±12 h.
pub fn parse_date_field(s: &str) -> Option<Date> {
    let dt = OffsetDateTime::parse(s, &Rfc3339).ok()?;
    Some((dt + Duration::hours(12)).date())
}

/// The value written to a date field: UTC midnight of `date`.
pub fn format_date_field(date: Date) -> String {
    format!("{}T00:00:00Z", date)
}

/// The fixed field set every card needs, plus the order field and any card-setting fields.
pub fn fields_to_fetch(order_field: &str, card_fields: &[String]) -> Vec<String> {
    let mut out: Vec<String> = [
        fields::ID,
        fields::REV,
        fields::TYPE,
        fields::TITLE,
        fields::STATE,
        fields::BOARD_COLUMN,
        fields::BOARD_COLUMN_DONE,
        fields::BOARD_LANE,
        fields::ASSIGNED_TO,
        fields::TAGS,
        fields::PARENT,
        fields::AREA_PATH,
        fields::ITERATION_PATH,
        fields::CHANGED_DATE,
        fields::PRIORITY,
        fields::TARGET_DATE,
        fields::COMPLETED_WORK,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .chain(std::iter::once(order_field.to_owned()))
    .chain(card_fields.iter().cloned())
    .collect();
    let mut seen = std::collections::HashSet::new();
    out.retain(|f| seen.insert(f.clone()));
    out
}

/// Everyone assigned to at least one card, sorted by name.
pub fn people<'a>(cards: impl IntoIterator<Item = &'a Card>) -> Vec<Person> {
    let mut out: Vec<Person> = cards
        .into_iter()
        .filter_map(|c| c.assigned_to.clone())
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.unique_name == b.unique_name);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials() {
        let p = |n: &str| Person {
            display_name: n.into(),
            unique_name: String::new(),
        };
        assert_eq!(p("Ada Lovelace").initials(), "AL");
        assert_eq!(p("Grace Brewster Hopper").initials(), "GH");
        assert_eq!(p("plato").initials(), "P");
        assert_eq!(p("Łukasz Ąbc").initials(), "ŁĄ");
    }

    #[test]
    fn date_field_rounds_local_midnight() {
        let d = |s| parse_date_field(s).unwrap().to_string();
        assert_eq!(d("2026-11-19T23:00:00Z"), "2026-11-20");
        assert_eq!(d("2026-11-20T00:00:00Z"), "2026-11-20");
        assert_eq!(d("2026-11-20T05:00:00Z"), "2026-11-20");
        let date = parse_date_field("2026-11-20T00:00:00Z").unwrap();
        assert_eq!(format_date_field(date), "2026-11-20T00:00:00Z");
    }

    #[test]
    fn fields_are_deduplicated() {
        let f = fields_to_fetch(
            "Microsoft.VSTS.Common.StackRank",
            &["System.Title".into(), "X.Y".into()],
        );
        assert_eq!(f.iter().filter(|s| *s == "System.Title").count(), 1);
        assert!(f.contains(&"X.Y".to_owned()));
        assert!(f.contains(&"Microsoft.VSTS.Common.StackRank".to_owned()));
    }
}
