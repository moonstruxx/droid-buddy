#![deny(unsafe_code)]
//! The one `unsafe` site in the crate is the wgpu surface creation in
//! `gui::mod`, which carries its own `#[allow(unsafe_code)]` and a `SAFETY`
//! comment naming the invariant it relies on. Everything else is safe Rust,
//! so an address or memory defect in this codebase cannot be introduced
//! silently: it would first have to pass this lint.

pub mod app;
pub mod cli;
pub mod config;
pub mod diff;
pub mod events;
pub mod expression;
pub mod favorites;
pub mod geometry;
pub mod graph;
pub mod graph_anim;
pub mod graph_group;
pub mod graph_render;
pub mod gui;
pub mod handler;
pub mod help;
pub mod latency;
pub mod layout;
pub mod optimize;
pub mod panes;
pub mod patch;
pub mod performance;
pub mod physical;
pub mod platform;
pub mod plugin;
pub mod rack;
pub mod report;
pub mod schema;
pub mod store;
pub mod sysex;
#[cfg(test)]
pub mod test_support;
pub mod theme;
pub mod validation;

#[cfg(test)]
pub mod regression;
