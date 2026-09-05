//! Pure help-content module for the `?` help modal (design D3).
//!
//! No terminal dependency, matching `diff.rs`/`graph.rs`/`validation.rs`.
//! Owns the `HelpView` enum, the `active_view(&App)` mapping that mirrors the
//! handler priority chain, and the per-view `keybindings` tables. The
//! renderer and the tests share one source of truth for what each view's
//! keys are; adding a key means editing one table.

use crate::app::App;

/// Which surface's keybindings the help modal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpView {
    /// Main panels / physical view.
    Panels,
    /// Embedded source viewer (`g v`).
    Viewer,
    /// Signal-flow graph surface (`g g`).
    Graph,
    /// Quad concurrent view (`g q`).
    Quad,
    /// Validation modal (`e`).
    Validation,
    /// Optimizer menu (`g o`).
    Optimizer,
    /// File picker (`l`).
    Picker,
}

impl HelpView {
    /// Short title shown in the modal's border, mirroring the surface name.
    pub fn title(self) -> &'static str {
        match self {
            HelpView::Panels => "Panels / Physical",
            HelpView::Viewer => "Source Viewer",
            HelpView::Graph => "Signal-flow Graph",
            HelpView::Quad => "Quad View",
            HelpView::Validation => "Validation",
            HelpView::Optimizer => "Optimizer",
            HelpView::Picker => "File Picker",
        }
    }
}

/// The active view, mirroring the handler priority chain
/// (picker > validation > optimizer > graph > quad > viewer > panels).
pub fn active_view(app: &App) -> HelpView {
    if app.showing_picker {
        HelpView::Picker
    } else if app.showing_validation {
        HelpView::Validation
    } else if app.optimizer.is_some() {
        HelpView::Optimizer
    } else if app.showing_graph {
        HelpView::Graph
    } else if app.showing_quad {
        HelpView::Quad
    } else if app.showing_viewer {
        HelpView::Viewer
    } else {
        HelpView::Panels
    }
}

/// The keybinding rows for a view: `(key, description)` pairs.
pub fn keybindings(view: HelpView) -> Vec<(&'static str, &'static str)> {
    match view {
        HelpView::Panels => vec![
            ("l", "open file picker"),
            ("g v", "open source viewer"),
            ("g g", "open signal-flow graph"),
            ("g q", "open quad view"),
            ("g d", "diff against another patch"),
            ("g o", "open latency optimizer"),
            ("?", "show this help"),
            ("1-4", "shift groups"),
            ("+/-", "scale presets"),
            ("s", "toggle skeleton presentation"),
            ("arrows/wheel", "pan when rack overflows"),
            ("Enter/Space", "toggle component"),
            ("e", "edit label / validation modal"),
            ("p", "pause processing"),
            ("q", "quit"),
        ],
        HelpView::Viewer => vec![
            ("j/k", "scroll source"),
            ("Up/Down", "navigate occurrences"),
            ("Home/End", "jump to first/last occurrence"),
            ("t", "toggle raw/prettified"),
            ("Tab", "switch pane focus"),
            ("[/]", "adjust panels/source split"),
            ("e", "edit label"),
            ("Esc", "close viewer"),
            ("?", "show this help"),
        ],
        HelpView::Graph => vec![
            ("x", "toggle circuit processing"),
            ("p", "pin/unpin node"),
            ("c", "toggle latency coloring"),
            ("e", "edit label"),
            ("d", "diff overlay"),
            ("+/-", "camera zoom"),
            ("arrows", "pan camera"),
            ("[/]", "cable tension"),
            ("Esc", "close graph"),
            ("?", "show this help"),
        ],
        HelpView::Quad => vec![
            ("Tab", "switch pane focus"),
            ("t", "toggle raw/prettified (source)"),
            ("[/]", "adjust panels/source split"),
            ("Up/Down/Home/End", "navigate occurrences"),
            ("e", "edit label"),
            ("Esc", "close quad"),
            ("?", "show this help"),
        ],
        HelpView::Validation => vec![
            ("j/k", "navigate issues"),
            ("Enter", "jump to source"),
            ("e", "toggle close"),
            ("Esc", "close"),
            ("?", "show this help"),
        ],
        HelpView::Optimizer => vec![
            ("j/k", "navigate candidates"),
            ("Enter", "preview"),
            ("r", "restore original order"),
            ("s", "export"),
            ("[/]", "adjust weight"),
            ("0/1", "snap weight"),
            ("Esc", "close"),
            ("?", "show this help"),
        ],
        HelpView::Picker => vec![
            ("j/k/arrows", "navigate"),
            ("Enter", "select"),
            ("f", "toggle favourite"),
            ("Esc", "close"),
            ("?", "show this help"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn active_view_defaults_to_panels() {
        assert_eq!(active_view(&app()), HelpView::Panels);
    }

    #[test]
    fn active_view_mirrors_priority_chain() {
        let mut a = app();
        a.showing_picker = true;
        assert_eq!(active_view(&a), HelpView::Picker);

        let mut a = app();
        a.showing_validation = true;
        assert_eq!(active_view(&a), HelpView::Validation);

        let mut a = app();
        a.patch = Some(
            crate::patch::Patch::from_ini_str("[button]\n    button = B1.1\n", String::from("t"))
                .unwrap(),
        );
        assert!(a.open_optimizer(), "optimizer needs a patch with sections");
        assert_eq!(active_view(&a), HelpView::Optimizer);

        let mut a = app();
        a.showing_graph = true;
        assert_eq!(active_view(&a), HelpView::Graph);

        let mut a = app();
        a.showing_quad = true;
        assert_eq!(active_view(&a), HelpView::Quad);

        let mut a = app();
        a.showing_viewer = true;
        assert_eq!(active_view(&a), HelpView::Viewer);
    }

    #[test]
    fn keybindings_non_empty_per_view() {
        for view in [
            HelpView::Panels,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Quad,
            HelpView::Validation,
            HelpView::Optimizer,
            HelpView::Picker,
        ] {
            let rows = keybindings(view);
            assert!(!rows.is_empty(), "view {view:?} must have bindings");
            for (key, desc) in &rows {
                assert!(!key.is_empty(), "view {view:?} has empty key");
                assert!(
                    !desc.is_empty(),
                    "view {view:?} has empty description for {key}"
                );
            }
        }
    }

    #[test]
    fn every_view_has_a_title() {
        for view in [
            HelpView::Panels,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Quad,
            HelpView::Validation,
            HelpView::Optimizer,
            HelpView::Picker,
        ] {
            assert!(!view.title().is_empty(), "view {view:?} must have a title");
        }
    }
}
