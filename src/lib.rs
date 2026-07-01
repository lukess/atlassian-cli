//! Library surface of atlassian-cli, exposing the Jira/Confluence client and
//! configuration loader for reuse by other crates (e.g. env-ops).
//!
//! The `atlassian` binary (`src/main.rs`) keeps its own private module tree;
//! this library intentionally exposes only the reusable, self-contained pieces.

pub mod config;
pub mod client;
