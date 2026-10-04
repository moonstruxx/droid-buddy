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
            ("click", "focus element across source + graph"),
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
            ("U", "upload patch to DROID (confirm modal)"),
            ("y", "send upload (confirm modal)"),
            ("n", "cancel upload (confirm modal)"),
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
            ("U", "upload patch to DROID (confirm modal)"),
            ("y", "send upload (confirm modal)"),
            ("n", "cancel upload (confirm modal)"),
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
            ("U", "upload patch to DROID (confirm modal)"),
            ("y", "send upload (confirm modal)"),
            ("n", "cancel upload (confirm modal)"),
            ("h", "toggle column/force layout"),
            ("a", "apply arrangement / cycle layouts"),
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
            ("U", "upload patch to DROID (confirm modal)"),
            ("y", "send upload (confirm modal)"),
            ("n", "cancel upload (confirm modal)"),
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
    fn keybindings_include_upload_keys() {
        // Change `midi-upload` task 2.1 (keybinding spec "The app SHALL bind
        // `U` ... and document all three in help"): `U` opens the upload
        // confirm modal from any pane view, and `y`/`n` send/cancel inside
        // it — so every pane view's table lists all three. The centered
        // overlays (picker, validation) never see the modal keys.
        let rows = [
            ("U", "upload patch to DROID (confirm modal)"),
            ("y", "send upload (confirm modal)"),
            ("n", "cancel upload (confirm modal)"),
        ];
        for view in [
            HelpView::ModuleUi,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Optimizer,
        ] {
            for row in rows {
                assert!(
                    keybindings(view).contains(&row),
                    "view {view:?} must document the upload key {}",
                    row.0
                );
            }
        }
        for view in [HelpView::Validation, HelpView::Picker] {
            assert!(
                !keybindings(view).iter().any(|(k, _)| *k == "U"),
                "view {view:?} must not document the U upload key"
            );
        }
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
    fn keybindings_reflect_graph_arrange_key() {
        // Change `graph-arrange-cycle` task 2.2: the graph table documents
        // the `a` arrange key (apply + cycle) next to the `h` layout toggle.
        let graph = keybindings(HelpView::Graph);
        assert!(graph.contains(&("a", "apply arrangement / cycle layouts")));
    }

    /// Every `(key, description)` pair the help modal can ever show, and the
    /// surface that owns it. Used by `module_ui_table_matches_handler` to
    /// catch un-documented rows and renamed descriptions in one assertion.
    const ALL_KNOWN_ROWS: &[(&str, &str)] = &[
        // Band / pane-layout keys (any pane view, both classes).
        ("z", "maximize focused pane"),
        ("Alt+b", "swap big pane view"),
        ("Alt+s", "swap small panes"),
        ("r", "cycle view in focused pane"),
        ("[/]", "adjust pane split"),
        ("Alt+[/Alt+]", "adjust small-pane split"),
        ("Tab/Shift+Tab", "cycle pane focus"),
        // Module UI (the physical faceplate surface).
        ("+/-", "zoom presets"),
        ("arrows/wheel", "pan rack on overflow"),
        ("arrows", "pan rack on overflow"),
        ("j/k", "navigate"),
        ("click", "focus element across source + graph"),
        ("Enter/Space", "toggle component"),
        ("s", "toggle skeleton presentation"),
        ("m", "latch modifier on hovered component"),
        ("e", "edit label / validation modal"),
        ("1-4", "shift groups"),
        ("Esc", "close Module UI view"),
        // Graph surface.
        ("x", "toggle circuit processing"),
        ("p", "pin/unpin node"),
        ("c", "center graph"),
        ("Shift+c", "fit and center graph"),
        ("e", "edit label"),
        ("d", "diff overlay"),
        ("h", "toggle column/force layout"),
        ("a", "apply arrangement / cycle layouts"),
        ("f", "dependency filter"),
        ("i", "influence filter"),
        ("+/-", "camera zoom"),
        ("arrows", "pan camera"),
        ("click minimap", "pan camera to clicked position"),
        ("Alt+[/Alt+]", "cable tension"),
        ("Esc", "close graph"),
        // Source viewer.
        ("j/k", "scroll source"),
        ("Up/Down", "navigate occurrences"),
        ("Home/End", "jump to first/last occurrence"),
        ("t", "toggle raw/prettified"),
        ("Esc", "close viewer"),
        // Validation modal.
        ("j/k", "navigate issues"),
        ("Enter", "jump to source"),
        ("e", "toggle close"),
        ("Esc", "close"),
        // Optimizer pane.
        ("j/k", "navigate candidates"),
        ("Enter", "preview"),
        ("r", "restore original order"),
        ("s", "export"),
        ("[/]", "adjust weight"),
        ("0/1", "snap weight"),
        // Picker overlay.
        ("j/k/arrows", "navigate"),
        ("Enter", "select"),
        ("f/F", "toggle favourite"),
        ("Ctrl+f", "toggle filter"),
        ("0-9", "fast-select favourite slot"),
        ("q", "end filter"),
        ("Backspace", "remove filter char"),
        ("type", "filter entries"),
        // Global keys present on most views.
        ("?", "show this help"),
        ("l", "open file picker"),
        ("g", "prefix mode (g v/g g/g d/g o/g c/g s)"),
        ("g s", "open select-state menu"),
        ("g c", "toggle latency coloring"),
        ("d", "toggle diff overlay"),
        ("p", "pause processing"),
        ("U", "upload patch to DROID (confirm modal)"),
        ("y", "send upload (confirm modal)"),
        ("n", "cancel upload (confirm modal)"),
        ("q", "quit"),
        ("Ctrl+c", "quit"),
    ];

    #[test]
    fn module_ui_table_matches_handler() {
        // `retire-panels-surface` task 3.1: the Module UI table must name the
        // keys the handler actually dispatches for the module-UI surface, and
        // nothing else. `panels_focused` routes the plain (non-prefix) dispatch
        // when neither graph, optimizer, nor viewer owns focus, so the exact
        // set below is the handler's Module UI contract (src/handler.rs).
        //
        // The merged table must contain every row the retired Panels and
        // Physical tables carried. Rows retired from the help content are the
        // ones that name surfaces the handler no longer dispatches: `\` split,
        // the `g q` quad chord, and the legacy `Panels`/`Physical Rack` titles.
        let table: Vec<(&str, &str)> = keybindings(HelpView::ModuleUi);
        let keys: Vec<&str> = table.iter().map(|(k, _)| *k).collect();

        for (key, desc) in [
            // Navigation, zoom, skeleton, select-state, focus (task 3.1 scope).
            ("j/k", "navigate"),
            ("click", "focus element across source + graph"),
            ("+/-", "zoom presets"),
            ("s", "toggle skeleton presentation"),
            ("g s", "open select-state menu"),
            ("Tab/Shift+Tab", "cycle pane focus"),
            // Component interaction and modifier/shift family.
            ("Enter/Space", "toggle component"),
            ("arrows/wheel", "pan rack on overflow"),
            ("m", "latch modifier on hovered component"),
            ("e", "edit label / validation modal"),
            ("1-4", "shift groups"),
            // Band and view management.
            ("z", "maximize focused pane"),
            ("Alt+b", "swap big pane view"),
            ("Alt+s", "swap small panes"),
            ("r", "cycle view in focused pane"),
            ("Esc", "close Module UI view"),
            // Global keys.
            ("?", "show this help"),
            ("l", "open file picker"),
            ("q", "quit"),
            ("Ctrl+c", "quit"),
            // MIDI upload (change `midi-upload`, task 2.1): `U` opens the
            // confirm modal from any pane view; `y`/`n` send/cancel in it.
            ("U", "upload patch to DROID (confirm modal)"),
            ("y", "send upload (confirm modal)"),
            ("n", "cancel upload (confirm modal)"),
        ] {
            assert!(
                table.contains(&(key, desc)),
                "Module UI table must name the handler-dispatched key {key:?}"
            );
        }

        // Every row is a known key; a new row must be added to ALL_KNOWN_ROWS
        // (and justified against the handler) or removed.
        for (key, desc) in table.iter() {
            assert!(
                ALL_KNOWN_ROWS.contains(&(*key, *desc)),
                "Module UI row {key:?} => {desc:?} is not a known handler binding"
            );
        }

        // Retired surfaces: the tiled left-pane split is gone (proposal
        // non-goal aside, the handler stretches to neither) and the quad chord
        // was retired with the tiling model.
        assert!(
            !keys.contains(&"\\"),
            "the tiled left-pane split must not remain in the Module UI table"
        );
        assert!(
            !keys.contains(&"g q"),
            "the quad view chord must not remain in the Module UI table"
        );
    }

    #[test]
    fn module_ui_table_has_exact_key_set() {
        // The Module UI table is the one place a key can be described for the
        // module-UI surface; an exact-set assertion makes an accidental drop or
        // an un-documented addition a hard failure. The set is derived from the
        // handler's plain dispatch plus the shared band/global keys.
        let mut keys: Vec<&str> = keybindings(HelpView::ModuleUi)
            .iter()
            .map(|(k, _)| *k)
            .collect();
        keys.sort_unstable();
        let mut expected = vec![
            "+/-",
            "1-4",
            "?",
            "Alt+b",
            "Alt+s",
            "Ctrl+c",
            "Enter/Space",
            "Esc",
            "click",
            "Tab/Shift+Tab",
            "arrows/wheel",
            "d",
            "e",
            "g",
            "g c",
            "g s",
            "j/k",
            "l",
            "m",
            "U",
            "n",
            "p",
            "q",
            "r",
            "s",
            "y",
            "z",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected, "Module UI key set drifted from the handler");
    }

    #[test]
    fn module_ui_surface_rows_name_real_handler_keys() {
        // Spec scenario "Panels table lists split, select-state, and focus
        // keys" (rewritten for the collapsed surface): the module-UI table
        // names the select-state chord and focus cycling. The split key is a
        // proposal-documented drift: the spec's scenario still names it, but
        // the handler no longer dispatches `\`, so the table must not carry it
        // and this test records that explicit decision.
        let table = keybindings(HelpView::ModuleUi);
        assert!(table.contains(&("g s", "open select-state menu")));
        assert!(table.contains(&("Tab/Shift+Tab", "cycle pane focus")));
        assert!(
            !table.iter().any(|(key, _)| *key == "\\"),
            "`\\` is not a handler binding, so it must not be documented"
        );
    }

    #[test]
    fn module_ui_physical_rows_are_merged_not_duplicated() {
        // `retire-panels-surface` 2.1 merges the old Panels and Physical tables
        // into one: each key appears at most once, and no row is left over from
        // a surface that no longer exists.
        let table = keybindings(HelpView::ModuleUi);
        let mut seen: Vec<&str> = table.iter().map(|(k, _)| *k).collect();
        seen.sort_unstable();
        let mut dedup = seen.clone();
        dedup.dedup();
        assert_eq!(seen, dedup, "Module UI table repeats a key");

        // No `Panels`/`Physical Rack` title survives in any describable view.
        for view in all_views() {
            assert_ne!(view.title(), "Panels");
            assert_ne!(view.title(), "Panels / Physical");
            assert_ne!(view.title(), "Physical Rack");
        }
    }

    fn all_views() -> [HelpView; 6] {
        [
            HelpView::ModuleUi,
            HelpView::Viewer,
            HelpView::Graph,
            HelpView::Validation,
            HelpView::Optimizer,
            HelpView::Picker,
        ]
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
