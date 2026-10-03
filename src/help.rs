//! Pure help-content module for the `?` help modal (design D3).
//!
//! No terminal dependency, matching `diff.rs`/`graph.rs`/`validation.rs`.
//! Owns the `HelpView` enum, the `active_view(&App)` mapping that mirrors the
//! handler priority chain, and the per-view `keybindings` tables. The
//! renderer and the tests share one source of truth for what each view's
//! keys are; adding a key means editing one table.

use crate::app::{App, ViewType};

/// Which surface's keybindings the help modal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpView {
    /// Module UI — the physical faceplate view (`ViewType::Physical`), the
    /// app's only hardware surface since `module-ui-physical` retired the
    /// Panels view.
    ModuleUi,
    /// Embedded source viewer (`g v`).
    Viewer,
    /// Signal-flow graph surface (`g g`).
    Graph,
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
            HelpView::ModuleUi => "Module UI",
            HelpView::Viewer => "Source Viewer",
            HelpView::Graph => "Signal-flow Graph",
            HelpView::Validation => "Validation",
            HelpView::Optimizer => "Optimizer",
            HelpView::Picker => "File Picker",
        }
    }
}

/// The active view, mirroring the handler priority chain.
///
/// Priority (change `pane-class-layout`): the centered overlays first
/// (picker > validation), then the class-based pane layout's focused pane. A
/// view resolves from the pane that holds it (`App.layout.focus`), so the
/// help modal describes the pane the user is actually working in — including
/// the optimizer, which is a small pane rather than an overlay modality. An
/// empty focused pane and the module UI both report `ModuleUi`.
pub fn active_view(app: &App) -> HelpView {
    if app.showing_picker {
        HelpView::Picker
    } else if app.showing_validation {
        HelpView::Validation
    } else {
        match app.layout.pane(app.layout.focus).view {
            Some(ViewType::Graph) => HelpView::Graph,
            Some(ViewType::SourceViewer) => HelpView::Viewer,
            Some(ViewType::Physical) => HelpView::ModuleUi,
            Some(ViewType::Optimizer) => HelpView::Optimizer,
            None => HelpView::ModuleUi,
        }
    }
}

/// The keybinding rows for a view: `(key, description)` pairs.
pub fn keybindings(view: HelpView) -> Vec<(&'static str, &'static str)> {
    match view {
        HelpView::ModuleUi => vec![
            ("+/-", "zoom presets"),
            ("arrows/wheel", "pan rack on overflow"),
            ("j/k", "navigate"),
            ("Enter/Space", "toggle component"),
            ("s", "toggle skeleton presentation"),
            ("m", "latch modifier on hovered component"),
            ("e", "edit label / validation modal"),
            ("1-4", "shift groups"),
            ("Tab/Shift+Tab", "cycle pane focus"),
            ("z", "maximize focused pane"),
            ("Alt+b", "swap big pane view"),
            ("Alt+s", "swap small panes"),
            ("r", "cycle view in focused pane"),
            ("Esc", "close Module UI view"),
            ("l", "open file picker"),
            ("g", "prefix mode (g v/g g/g d/g o/g c/g s)"),
            ("g s", "open select-state menu"),
            ("g c", "toggle latency coloring"),
            ("d", "toggle diff overlay"),
            ("p", "pause processing"),
            ("?", "show this help"),
            ("q", "quit"),
            ("Ctrl+c", "quit"),
        ],
        HelpView::Viewer => vec![
            ("j/k", "scroll source"),
            ("Up/Down", "navigate occurrences"),
            ("Home/End", "jump to first/last occurrence"),
            ("t", "toggle raw/prettified"),
            ("Tab/Shift+Tab", "cycle pane focus"),
            ("z", "maximize focused pane"),
            ("Alt+b", "swap big pane view"),
            ("Alt+s", "swap small panes"),
            ("r", "cycle view in focused pane"),
            ("[/]", "adjust pane split"),
            ("e", "edit label"),
            ("Esc", "close viewer"),
            ("?", "show this help"),
            ("l", "open file picker"),
            ("g", "prefix mode (g v/g g/g d/g o/g c/g s)"),
            ("p", "pause processing"),
            ("1-4", "shift groups"),
            ("m", "latch modifier"),
            ("d", "toggle diff overlay"),
            ("q", "quit"),
            ("Ctrl+c", "quit"),
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
            ("click minimap", "pan camera to clicked position"),
            ("Alt+[/Alt+]", "cable tension"),
            ("z", "maximize focused pane"),
            ("Alt+b", "swap big pane view"),
            ("Alt+s", "swap small panes"),
            ("r", "cycle view in focused pane"),
            ("Esc", "close graph"),
            ("?", "show this help"),
            ("l", "open file picker"),
            ("q", "quit"),
            ("Ctrl+c", "quit"),
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
            ("z", "maximize focused pane"),
            ("Alt+b", "swap big pane view"),
            ("Alt+s", "swap small panes"),
            ("Esc", "close"),
            ("?", "show this help"),
            ("l", "open file picker"),
            ("q", "quit"),
            ("Ctrl+c", "quit"),
            ("p", "pause processing"),
        ],
        HelpView::Picker => vec![
            ("j/k/arrows", "navigate"),
            ("Enter", "select"),
            ("f/F", "toggle favourite"),
            ("Ctrl+f", "toggle filter"),
            ("0-9", "fast-select favourite slot"),
            ("q", "end filter"),
            ("Backspace", "remove filter char"),
            ("type", "filter entries"),
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
    fn active_view_defaults_to_module_ui() {
        // Startup layout (spec "Startup pane configuration"): focus starts on
        // the left big pane, which holds the module UI (Physical), so `?` reports ModuleUi.
        let a = app();
        assert_eq!(a.layout.focus, crate::panes::PaneId::BigLeft);
        assert_eq!(active_view(&a), HelpView::ModuleUi);
    }

    #[test]
    fn active_view_mirrors_priority_chain() {
        // Overlays keep the top priority (spec "Overlays render above the pane
        // layout"): the picker and the validation modal outrank the pane layout.
        let mut a = app();
        a.showing_picker = true;
        assert_eq!(active_view(&a), HelpView::Picker);

        let mut a = app();
        a.showing_validation = true;
        assert_eq!(active_view(&a), HelpView::Validation);

        // Below the overlays the active view is the focused pane's view, in
        // every class: big (graph / physical) and small (optimizer / viewer).
        let mut a = app();
        a.open_view(ViewType::Graph);
        assert_eq!(a.layout.pane(a.layout.focus).view, Some(ViewType::Graph));
        assert_eq!(active_view(&a), HelpView::Graph);

        let mut a = app();
        a.open_view(ViewType::Physical);
        assert_eq!(active_view(&a), HelpView::ModuleUi);

        // The optimizer is a small pane, not an overlay: opening it focuses
        // that pane and `?` describes it.
        let mut a = app();
        a.patch = Some(
            crate::patch::Patch::from_ini_str("[button]\n    button = B1.1\n", String::from("t"))
                .unwrap(),
        );
        assert!(a.open_optimizer(), "optimizer needs a patch with sections");
        assert_eq!(active_view(&a), HelpView::Optimizer);

        // The source viewer is the second small-class view; the startup layout
        // already places it, so focusing its pane reports it.
        let mut a = app();
        let viewer_pane = a
            .pane_holding(ViewType::SourceViewer)
            .expect("startup layout holds the source viewer");
        a.layout.focus = viewer_pane;
        assert_eq!(active_view(&a), HelpView::Viewer);
    }

    #[test]
    fn keybindings_non_empty_per_view() {
        for view in [
            HelpView::ModuleUi,
            HelpView::Viewer,
            HelpView::Graph,
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
        // `r` rotates the focused pane through its class carousel (big: graph
        // / module UI / physical; small: source viewer / optimizer) on every
        // pane view; the full-surface overlays (validation) and the picker do
        // not hold a pane. The optimizer's own table owns `r` (restore).
        let row = ("r", "cycle view in focused pane");
        for view in [HelpView::ModuleUi, HelpView::Viewer, HelpView::Graph] {
            assert!(
                keybindings(view).contains(&row),
                "view {view:?} must document the r carousel key"
            );
        }
        for view in [HelpView::Validation, HelpView::Picker] {
            assert!(
                !keybindings(view).contains(&row),
                "view {view:?} must not document the r carousel key"
            );
        }
        // The optimizer keeps `r` as restore, not the class carousel.
        let optimizer = keybindings(HelpView::Optimizer);
        assert!(optimizer.contains(&("r", "restore original order")));
        assert!(!optimizer.contains(&row));
    }

    #[test]
    fn keybindings_include_pane_layout_keys() {
        // Change `pane-class-layout` task 3.3 (keybinding spec "The help modal
        // tables SHALL list all three keys"): `z`, `Alt+b`, and `Alt+s` act on
        // the band from any focused pane, so every pane view's table lists
        // them. The centered overlays (picker, validation) do not.
        let z = ("z", "maximize focused pane");
        let alt_b = ("Alt+b", "swap big pane view");
        let alt_s = ("Alt+s", "swap small panes");
        for view in [
            HelpView::ModuleUi,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Optimizer,
        ] {
            let rows = keybindings(view);
            for row in [z, alt_b, alt_s] {
                assert!(rows.contains(&row), "view {view:?} must document {row:?}");
            }
        }
        for view in [HelpView::Validation, HelpView::Picker] {
            let rows = keybindings(view);
            for row in [z, alt_b, alt_s] {
                assert!(
                    !rows.contains(&row),
                    "view {view:?} must not document {row:?}"
                );
            }
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

        let module_ui = keybindings(HelpView::ModuleUi);
        assert!(module_ui.contains(&("g c", "toggle latency coloring")));
    }

    #[test]
    fn keybindings_reflect_module_ui_surface() {
        // Change `help-keybinding-parity` task 1.3 + `retire-panels-surface`:
        // the Module UI table documents the zoom presets, overflow pan (arrows
        // + wheel), the j/k navigation the surface really binds, the skeleton
        // toggle, and Esc closing the view.
        let module_ui = keybindings(HelpView::ModuleUi);
        assert!(module_ui.contains(&("+/-", "zoom presets")));
        assert!(module_ui.contains(&("arrows/wheel", "pan rack on overflow")));
        assert!(module_ui.contains(&("j/k", "navigate")));
        assert!(module_ui.contains(&("s", "toggle skeleton presentation")));
        assert!(module_ui.contains(&("Esc", "close Module UI view")));
    }

    #[test]
    fn keybindings_reflect_module_ui_pane_keys() {
        // Change `help-keybinding-parity` task 1.1 + `pane-class-layout` 3.3 +
        // `retire-panels-surface`: the Module UI table lists the select-state
        // chord, pane-focus cycling, and the pane-layout keys. The tiled
        // left-pane split and the quad chord are retired with the tiling model.
        let module_ui = keybindings(HelpView::ModuleUi);
        assert!(module_ui.contains(&("g s", "open select-state menu")));
        assert!(module_ui.contains(&("Tab/Shift+Tab", "cycle pane focus")));
        assert!(module_ui.contains(&("z", "maximize focused pane")));
        assert!(
            !module_ui.contains(&("\\", "toggle left-pane vertical split")),
            "the tiled left-pane split is retired"
        );
        assert!(
            !module_ui.iter().any(|(key, _)| *key == "g q"),
            "the quad view is retired"
        );
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
        // Change `help-keybinding-parity` task 1.2 + `pane-class-layout`:
        // the graph table documents the layout toggle, dependency/influence
        // filters, the select-state chord, and the Alt-modified cable-tension
        // binding (plain brackets adjust the pane split in the class layout).
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
            HelpView::ModuleUi,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Validation,
            HelpView::Optimizer,
            HelpView::Picker,
        ] {
            assert!(!view.title().is_empty(), "view {view:?} must have a title");
        }
    }
}
