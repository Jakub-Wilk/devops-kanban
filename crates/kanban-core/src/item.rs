//! The work item behind the new/edit dialog: its values ([`ItemDraft`]), which fields differ
//! from what ADO has (the unsaved-changes list and the PATCH), and rebasing the user's edits
//! onto a newer revision after a conflict, so they stay on top of the other person's changes
//! instead of overwriting them.

use serde_json::Value;
use time::Date;

use crate::card::{Card, CardId, fields, format_date_field};

/// Longest title ADO accepts.
pub const TITLE_MAX: usize = 255;

/// One editable field of a work item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemField {
    Title,
    Type,
    Parent,
    Tags,
    Description,
    State,
    Priority,
    Assignee,
    TargetDate,
    Area,
}

impl ItemField {
    /// Dialog order: left column, then the side column.
    pub const ALL: [Self; 10] = [
        Self::Title,
        Self::Type,
        Self::Parent,
        Self::Tags,
        Self::Description,
        Self::State,
        Self::Priority,
        Self::Assignee,
        Self::TargetDate,
        Self::Area,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Type => "Type",
            Self::Parent => "Parent",
            Self::Tags => "Tags",
            Self::Description => "Description",
            Self::State => "State",
            Self::Priority => "Priority",
            Self::Assignee => "Assigned to",
            Self::TargetDate => "Target date",
            Self::Area => "Area",
        }
    }
}

/// The dialog's values for one work item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemDraft {
    pub title: String,
    pub work_item_type: String,
    pub parent: Option<CardId>,
    pub tags: Vec<String>,
    /// HTML, as `System.Description` stores it.
    pub description: String,
    pub state: String,
    /// 1 (critical) – 4 (low).
    pub priority: u8,
    /// `unique_name`; empty = unassigned.
    pub assignee: String,
    pub target_date: Option<Date>,
    pub area: String,
}

impl ItemDraft {
    pub fn from_card(card: &Card, description: String) -> Self {
        Self {
            title: card.title.clone(),
            work_item_type: card.work_item_type.clone(),
            parent: card.parent,
            tags: card.tags.clone(),
            description,
            state: card.state.clone(),
            priority: card.priority.unwrap_or(2),
            assignee: card
                .assigned_to
                .as_ref()
                .map(|p| p.unique_name.clone())
                .unwrap_or_default(),
            target_date: card.target_date,
            area: card.area_path.clone(),
        }
    }

    fn same(&self, other: &Self, field: ItemField) -> bool {
        match field {
            ItemField::Title => self.title.trim() == other.title.trim(),
            ItemField::Type => self.work_item_type == other.work_item_type,
            ItemField::Parent => self.parent == other.parent,
            ItemField::Tags => same_tags(&self.tags, &other.tags),
            ItemField::Description => self.description == other.description,
            ItemField::State => self.state == other.state,
            ItemField::Priority => self.priority == other.priority,
            ItemField::Assignee => self.assignee == other.assignee,
            ItemField::TargetDate => self.target_date == other.target_date,
            ItemField::Area => self.area == other.area,
        }
    }

    fn take(&mut self, from: &Self, field: ItemField) {
        match field {
            ItemField::Title => self.title.clone_from(&from.title),
            ItemField::Type => self.work_item_type.clone_from(&from.work_item_type),
            ItemField::Parent => self.parent = from.parent,
            ItemField::Tags => self.tags.clone_from(&from.tags),
            ItemField::Description => self.description.clone_from(&from.description),
            ItemField::State => self.state.clone_from(&from.state),
            ItemField::Priority => self.priority = from.priority,
            ItemField::Assignee => self.assignee.clone_from(&from.assignee),
            ItemField::TargetDate => self.target_date = from.target_date,
            ItemField::Area => self.area.clone_from(&from.area),
        }
    }

    /// Fields that differ from `original`, in dialog order.
    pub fn changed_fields(&self, original: &Self) -> Vec<ItemField> {
        ItemField::ALL
            .into_iter()
            .filter(|f| !self.same(original, *f))
            .collect()
    }

    /// After a conflict: `old` is what the dialog was opened on, `new` the server's newer
    /// revision. Fields the user left alone take the server's value; fields they edited keep
    /// theirs. Returns the fields the other person changed.
    pub fn rebase(&mut self, old: &Self, new: &Self) -> Vec<ItemField> {
        let theirs = new.changed_fields(old);
        for f in &theirs {
            if self.same(old, *f) {
                self.take(new, *f);
            }
        }
        theirs
    }

    /// The field writes for a PATCH: every changed field against `original`, or, when creating
    /// (`None`), every field with a value. Type and parent are not fields (the type is fixed
    /// once created; the parent is a relation), so they are never included.
    pub fn patch_fields(&self, original: Option<&Self>) -> Vec<(String, Value)> {
        let changed = |f: ItemField| original.is_none_or(|o| !self.same(o, f));
        let creating = original.is_none();
        let mut out: Vec<(String, Value)> = Vec::new();
        let mut put = |name: &str, value: Value| out.push((name.to_owned(), value));
        if changed(ItemField::Title) {
            put(fields::TITLE, Value::from(self.title.trim()));
        }
        if changed(ItemField::Tags) && !(creating && self.tags.is_empty()) {
            put(fields::TAGS, Value::from(tags_value(&self.tags)));
        }
        if changed(ItemField::Description) && !(creating && is_blank_html(&self.description)) {
            put(fields::DESCRIPTION, Value::from(self.description.clone()));
        }
        if changed(ItemField::State) && !self.state.is_empty() {
            put(fields::STATE, Value::from(self.state.clone()));
        }
        if changed(ItemField::Priority) {
            put(fields::PRIORITY, Value::from(self.priority));
        }
        if changed(ItemField::Assignee) && !(creating && self.assignee.is_empty()) {
            let v = if self.assignee.is_empty() {
                Value::Null
            } else {
                Value::from(self.assignee.clone())
            };
            put(fields::ASSIGNED_TO, v);
        }
        if changed(ItemField::TargetDate) && !(creating && self.target_date.is_none()) {
            let v = self
                .target_date
                .map_or(Value::Null, |d| Value::from(format_date_field(d)));
            put(fields::TARGET_DATE, v);
        }
        if changed(ItemField::Area) && !self.area.is_empty() {
            put(fields::AREA_PATH, Value::from(self.area.clone()));
        }
        out
    }
}

/// `System.Tags` value: `"; "`-separated.
pub fn tags_value(tags: &[String]) -> String {
    tags.join("; ")
}

/// Adds `tag` unless it is already there (case-insensitively). ADO stores tags without `;`.
pub fn add_tag(tags: &mut Vec<String>, tag: &str) {
    let tag = tag.replace(';', " ");
    let tag = tag.trim();
    if !tag.is_empty() && !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
        tags.push(tag.to_owned());
    }
}

/// Is `tag` among `tags` (case-insensitively, as ADO compares them)?
pub fn has_tag(tags: &[String], tag: &str) -> bool {
    tags.iter()
        .any(|t| t.trim().eq_ignore_ascii_case(tag.trim()))
}

/// Removes `tag` (case-insensitively).
pub fn remove_tag(tags: &mut Vec<String>, tag: &str) {
    tags.retain(|t| !t.trim().eq_ignore_ascii_case(tag.trim()));
}

fn same_tags(a: &[String], b: &[String]) -> bool {
    let key = |t: &[String]| {
        let mut v: Vec<String> = t.iter().map(|s| s.trim().to_lowercase()).collect();
        v.sort();
        v
    };
    key(a) == key(b)
}

/// No text and no images: an empty editor, or ADO's empty `<div></div>`.
pub fn is_blank_html(html: &str) -> bool {
    if html.contains("<img") {
        return false;
    }
    let mut in_tag = false;
    let mut text = String::new();
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ").trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn draft() -> ItemDraft {
        ItemDraft {
            title: "Render variants".into(),
            work_item_type: "User Story".into(),
            parent: Some(7),
            tags: vec!["print".into(), "performance".into()],
            description: "<p>Hi</p>".into(),
            state: "Active".into(),
            priority: 1,
            assignee: "me@x".into(),
            target_date: Some(date!(2026 - 11 - 12)),
            area: "Hubert\\Print".into(),
        }
    }

    #[test]
    fn changes_ignore_tag_order_and_case() {
        let original = draft();
        let mut d = draft();
        d.tags = vec!["Performance".into(), "print".into()];
        assert!(d.changed_fields(&original).is_empty());
        d.state = "Blocked".into();
        d.priority = 2;
        assert_eq!(
            d.changed_fields(&original),
            [ItemField::State, ItemField::Priority]
        );
    }

    #[test]
    fn patch_has_only_changed_fields() {
        let original = draft();
        let mut d = draft();
        d.state = "Blocked".into();
        d.assignee.clear();
        add_tag(&mut d.tags, "customer-facing");
        let names: Vec<String> = d
            .patch_fields(Some(&original))
            .into_iter()
            .map(|(f, _)| f)
            .collect();
        assert_eq!(
            names,
            [fields::TAGS, fields::STATE, fields::ASSIGNED_TO].map(str::to_owned)
        );
        let tags = d.patch_fields(Some(&original))[0].1.clone();
        assert_eq!(tags, "print; performance; customer-facing");
        // Unassigning writes null.
        assert_eq!(d.patch_fields(Some(&original))[2].1, Value::Null);
    }

    #[test]
    fn create_skips_empty_values() {
        let d = ItemDraft {
            title: " New ".into(),
            work_item_type: "Task".into(),
            description: "<div>&nbsp;</div>".into(),
            state: "New".into(),
            priority: 2,
            area: "Hubert".into(),
            ..ItemDraft::default()
        };
        let names: Vec<String> = d.patch_fields(None).into_iter().map(|(f, _)| f).collect();
        assert_eq!(
            names,
            [
                fields::TITLE,
                fields::STATE,
                fields::PRIORITY,
                fields::AREA_PATH
            ]
            .map(str::to_owned)
        );
        assert_eq!(d.patch_fields(None)[0].1, "New");
    }

    #[test]
    fn rebase_keeps_mine_and_takes_theirs() {
        let old = draft();
        let mut mine = draft();
        mine.state = "Blocked".into();
        mine.priority = 2;
        let mut theirs = draft();
        theirs.target_date = Some(date!(2026 - 11 - 20));
        theirs.priority = 3;
        let changed = mine.rebase(&old, &theirs);
        assert_eq!(changed, [ItemField::Priority, ItemField::TargetDate]);
        // Their date (I didn't touch it), my state and my priority (I changed both).
        assert_eq!(mine.target_date, Some(date!(2026 - 11 - 20)));
        assert_eq!(mine.state, "Blocked");
        assert_eq!(mine.priority, 2);
        // Saving now writes only what differs from their version.
        let names: Vec<String> = mine
            .patch_fields(Some(&theirs))
            .into_iter()
            .map(|(f, _)| f)
            .collect();
        assert_eq!(names, [fields::STATE, fields::PRIORITY].map(str::to_owned));
    }

    #[test]
    fn tags_dedupe_and_drop_separators() {
        let mut t = vec!["UI".to_owned()];
        add_tag(&mut t, "ui");
        add_tag(&mut t, "  ");
        add_tag(&mut t, "a;b");
        assert_eq!(t, ["UI", "a b"]);
        assert!(has_tag(&t, " ui "));
        remove_tag(&mut t, "Ui");
        assert_eq!(t, ["a b"]);
    }

    #[test]
    fn blank_html() {
        assert!(is_blank_html(""));
        assert!(is_blank_html("<div><br></div>"));
        assert!(is_blank_html("<p>&nbsp; </p>"));
        assert!(!is_blank_html("<p>x</p>"));
        assert!(!is_blank_html("<img src=\"a.png\">"));
    }
}
