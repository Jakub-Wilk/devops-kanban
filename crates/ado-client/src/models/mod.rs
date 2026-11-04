//! Serde DTOs mirroring ADO JSON (camelCase). Kept close to the wire format; the domain model
//! lives in `kanban-core`.

pub mod board;
pub mod identity;
pub mod patch;
pub mod workitem;
