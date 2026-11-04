//! Endpoint wrappers, one module per API area. Each adds `impl AdoClient { ... }` methods.

pub mod avatars;
pub mod backlog;
pub mod boards;
pub mod comments;
pub mod connection;
pub mod iterations;
pub mod order;
pub mod teams;
pub mod wiql;
pub mod workitems;
