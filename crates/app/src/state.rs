//! App-wide reactive state, provided via context at the root.
//!
//! The loaded board lives in a Dioxus 0.7 `Store` so a card update re-renders only that card.
//! Holds: settings, connection status, current board, in-flight moves (for optimistic updates
//! and rollback), last sync time, and the transient drag state (dragged card id).
