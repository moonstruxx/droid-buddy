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
    /// Physical rack tile (`s`).
    Physical,
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
            HelpView::Physical => "Physical Rack",
            HelpView::Validation => "Validation",
            HelpView::Optimizer => "Optimizer",
            HelpView::Picker => "File Picker",
        }
    }
}

/// The active view, mirroring the handler priority chain
/// (picker > validation > optimizer > graph > viewer > panels). The Physical
/// slot has no legacy `showing_*` flag, so it resolves from tile focus
/// directly — a focused Physical slot outranks the legacy graph/viewer bools
/// so `?` describes the surface the user is actually on.
pub fn active_view(app: &App) -> HelpView {
    if app.showing_picker {
        HelpView::Picker
    } else if app.showing_validation {
        HelpView::Validation
    } else if app.optimizer.is_some() {
        HelpView::Optimizer
    } else if matches!(
        app.tile_stack.focus,
        crate::app::FocusSlot::Slot(i)
            if app.tile_stack.slots.get(i) == Some(&crate::app::ViewType::Physical)
    ) {
        HelpView::Physical
    } else if app.showing_graph {
        HelpView::Graph
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
            ("g d", "diff against another patch"),
            ("g o", "open latency optimizer"),
            ("g c", "toggle latency coloring"),
            ("g s", "open select-state menu"),
            ("g q", "quad Panels/Source/Graph FULL/FILTERED"),
            ("\\", "toggle left-pane vertical split"),
            (
                "r",
                "cycle view in focused slot (graph / source / physical)",
            ),
            ("Tab/Shift+Tab", "cycle pane focus"),
            ("?", "show this help"),
            ("1-4", "shift groups"),
            ("+/-", "scale presets"),
            ("s", "open Physical tile slot"),
            ("arrows/wheel", "pan when rack overflows"),
            ("Enter/Space", "toggle component"),
            ("e", "edit label / validation modal"),
            ("m", "latch modifier on hovered component"),
            ("p", "pause processing"),
            ("q", "quit"),
        ],
        HelpView::Viewer => vec![
            ("j/k", "scroll source"),
            ("Up/Down", "navigate occurrences"),
            ("Home/End", "jump to first/last occurrence"),
            ("t", "toggle raw/prettified"),
            ("Tab", "switch pane focus"),
            (
                "r",
                "cycle view in focused slot (graph / source / physical)",
            ),
            ("[/]", "adjust panels/source split"),
            ("e", "edit label"),
            ("Esc", "close viewer"),
            ("?", "show this help"),
        ],
        HelpView::Graph => vec![
            ("x", "toggle circuit processing"),
            ("p", "pin/unpin node"),
            ("c", "center graph"),
            ("Shift+c", "fit and center graph"),
            ("e", "edit label"),
            ("d", "diff overlay"),
            ("h", "toggle column/force layout"),
            ("f", "dependency filter"),
            ("i", "influence filter"),
            ("g s", "open select-state menu"),
            ("+/-", "camera zoom"),
            ("arrows", "pan camera"),
            ("Alt+[/Alt+]", "cable tension"),
            (
                "r",
                "cycle view in focused slot (graph / source / physical)",
            ),
            ("Esc", "close graph"),
            ("?", "show this help"),
        ],
        HelpView::Physical => vec![
            ("+/-", "zoom presets"),
            ("arrows/wheel", "pan rack on overflow"),
            ("j/k", "navigate"),
            ("s", "toggle skeleton presentation"),
            (
                "r",
                "cycle view in focused slot (graph / source / physical)",
            ),
            ("Esc", "close Physical tile"),
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
            ("Ctrl+f", "toggle filter"),
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
        a.tile_stack.open(crate::app::ViewType::Physical);
        a.tile_stack.focus = crate::app::FocusSlot::Slot(0);
        assert_eq!(active_view(&a), HelpView::Physical);

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
            HelpView::Physical,
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
    fn keybindings_include_carousel_key() {
        // `r` rotates the focused right-column slot through the carousel on
        // every surface that can hold a slot; the optimizer is excluded.
        let row = (
            "r",
            "cycle view in focused slot (graph / source / physical)",
        );
        for view in [
            HelpView::Panels,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Physical,
        ] {
            assert!(
                keybindings(view).contains(&row),
                "view {view:?} must document the r carousel key"
            );
        }
        for view in [HelpView::Validation, HelpView::Optimizer, HelpView::Picker] {
            assert!(
                !keybindings(view).contains(&row),
                "view {view:?} must not document the r carousel key"
            );
        }
    }

    #[test]
    fn keybindings_reflect_graph_center_fit_and_g_c_chord() {
        // Design D3/D5: bare `c` centers the graph, `Shift+c` refits, and
        // latency coloring lives on the `g c` chord in the panels table.
        let graph = keybindings(HelpView::Graph);
        assert!(graph.contains(&("c", "center graph")));
        assert!(graph.contains(&("Shift+c", "fit and center graph")));
        assert!(
            !graph.contains(&("c", "toggle latency coloring")),
            "latency coloring must not remain on bare c in the graph table"
        );

        let panels = keybindings(HelpView::Panels);
        assert!(panels.contains(&("g c", "toggle latency coloring")));
    }

    #[test]
    fn keybindings_reflect_physical_surface() {
        // Change `help-keybinding-parity` task 1.3: the Physical tile table
        // documents the zoom presets, overflow pan (arrows + wheel), and the
        // j/k navigation the surface really binds.
        let physical = keybindings(HelpView::Physical);
        assert!(physical.contains(&("+/-", "zoom presets")));
        assert!(physical.contains(&("arrows/wheel", "pan rack on overflow")));
        assert!(physical.contains(&("j/k", "navigate")));
        assert!(physical.contains(&("s", "toggle skeleton presentation")));
        assert!(physical.contains(&("Esc", "close Physical tile")));
    }

    #[test]
    fn keybindings_reflect_panels_surface() {
        // Change `help-keybinding-parity` task 1.1: the Panels table lists the
        // left-pane split toggle, the select-state chord, and pane-focus cycling.
        let panels = keybindings(HelpView::Panels);
        assert!(panels.contains(&("\\", "toggle left-pane vertical split")));
        assert!(panels.contains(&("g s", "open select-state menu")));
        assert!(panels.contains(&("Tab/Shift+Tab", "cycle pane focus")));
    }

    #[test]
    fn keybindings_reflect_picker_filter() {
        // Change `help-keybinding-parity` task 1.4: the Picker table documents
        // Ctrl+f for the filter, distinct from bare `f` (favourite toggle).
        let picker = keybindings(HelpView::Picker);
        assert!(picker.contains(&("Ctrl+f", "toggle filter")));
    }

    #[test]
    fn keybindings_reflect_graph_filters_and_tension_binding() {
        // Change `help-keybinding-parity` task 1.2: the graph table documents
        // the layout toggle, dependency/influence filters, the select-state
        // chord, and the Alt-modified cable-tension binding (plain brackets
        // adjust the tiled split).
        let graph = keybindings(HelpView::Graph);
        assert!(graph.contains(&("h", "toggle column/force layout")));
        assert!(graph.contains(&("f", "dependency filter")));
        assert!(graph.contains(&("i", "influence filter")));
        assert!(graph.contains(&("g s", "open select-state menu")));
        assert!(graph.contains(&("Alt+[/Alt+]", "cable tension")));
        assert!(
            !graph.contains(&("[/]", "cable tension")),
            "bare brackets no longer adjust cable tension"
        );
    }

    #[test]
    fn every_view_has_a_title() {
        for view in [
            HelpView::Panels,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Physical,
            HelpView::Validation,
            HelpView::Optimizer,
            HelpView::Picker,
        ] {
            assert!(!view.title().is_empty(), "view {view:?} must have a title");
        }
    }
}
