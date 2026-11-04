//! JSON Patch document builder for work item updates (`test` / `add` / `replace` / `remove`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOp {
    pub op: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// An ordered JSON Patch. Updates start with `test /rev` (see [`PatchDocument::update`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatchDocument(pub Vec<PatchOp>);

impl PatchDocument {
    /// A document for creating a work item (no revision test).
    pub fn create() -> Self {
        Self::default()
    }

    /// A document for updating a work item at `rev`: the first op is always `test /rev`.
    pub fn update(rev: u32) -> Self {
        Self(vec![PatchOp {
            op: "test".into(),
            path: "/rev".into(),
            value: Some(json!(rev)),
        }])
    }

    /// Sets a field; `Value::Null` removes it.
    pub fn field(mut self, reference_name: &str, value: Value) -> Self {
        let path = format!("/fields/{reference_name}");
        self.0.push(if value.is_null() {
            PatchOp {
                op: "remove".into(),
                path,
                value: None,
            }
        } else {
            PatchOp {
                op: "add".into(),
                path,
                value: Some(value),
            }
        });
        self
    }

    /// Links the item to `parent_url` as its parent (`System.LinkTypes.Hierarchy-Reverse`).
    pub fn parent(mut self, parent_url: &str) -> Self {
        self.0.push(PatchOp {
            op: "add".into(),
            path: "/relations/-".into(),
            value: Some(json!({ "rel": "System.LinkTypes.Hierarchy-Reverse", "url": parent_url })),
        });
        self
    }

    /// Removes the relation at `index` (its position in the item's `relations`).
    pub fn remove_relation(mut self, index: usize) -> Self {
        self.0.push(PatchOp {
            op: "remove".into(),
            path: format!("/relations/{index}"),
            value: None,
        });
        self
    }

    pub fn is_empty_update(&self) -> bool {
        self.0.iter().all(|op| op.op == "test")
    }

    /// The revision tested by this document, if any.
    pub fn tested_rev(&self) -> Option<u32> {
        self.0
            .iter()
            .find(|op| op.op == "test" && op.path == "/rev")
            .and_then(|op| op.value.as_ref()?.as_u64())
            .map(|v| v as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_starts_with_rev_test() {
        let doc = PatchDocument::update(7).field("System.State", json!("Active"));
        let v = serde_json::to_value(&doc).unwrap();
        assert_eq!(
            v,
            json!([
                { "op": "test", "path": "/rev", "value": 7 },
                { "op": "add", "path": "/fields/System.State", "value": "Active" }
            ])
        );
        assert_eq!(doc.tested_rev(), Some(7));
    }

    #[test]
    fn reparent_removes_then_adds() {
        let doc = PatchDocument::update(3)
            .remove_relation(1)
            .parent("https://dev.azure.com/o/_apis/wit/workItems/9");
        let v = serde_json::to_value(&doc).unwrap();
        assert_eq!(v[1], json!({ "op": "remove", "path": "/relations/1" }));
        assert_eq!(v[2]["value"]["rel"], "System.LinkTypes.Hierarchy-Reverse");
    }

    #[test]
    fn null_removes_field() {
        let doc =
            PatchDocument::update(1).field("Microsoft.VSTS.Scheduling.TargetDate", Value::Null);
        assert_eq!(doc.0[1].op, "remove");
        assert!(doc.0[1].value.is_none());
    }
}
