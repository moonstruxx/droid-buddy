pub mod app;
pub mod config;
pub mod diff;
pub mod events;
pub mod favorites;
pub mod gallery;
pub mod geometry;
pub mod graph;
pub mod graph_render;
pub mod handler;
pub mod kitty_protocol;
pub mod latency;
pub mod layout;
pub mod optimize;
pub mod patch;
pub mod physical;
pub mod plugin;
pub mod rendermetrics;
pub mod schema;
pub mod theme;
pub mod ui;
pub mod validation;

#[cfg(test)]
pub mod regression;
