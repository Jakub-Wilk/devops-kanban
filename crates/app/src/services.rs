//! Bridge between UI and `kanban-core`: builds the `AdoClient` from settings + keyring, runs
//! loads/moves as async tasks, applies optimistic updates to the store and rolls back on
//! `AdoError::Conflict` / failures, and owns the `SyncEngine` background task.
