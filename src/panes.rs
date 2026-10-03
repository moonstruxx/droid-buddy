//! Class-based pane layout for the main band (change `pane-class-layout`).

use crate::app::ViewType;

/// Window class of a view: the panes it may occupy and the right-half shape
/// its presence selects (design D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneClass {
    /// Full-height pane: the left pane, or the right pane when no small view
    /// is open.
    Big,
    /// Quarter-height pane of the split right half.
    Small,
}

/// The four panes of the adaptive band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneId {
    BigLeft,
    BigRight,
    SmallTop,
    SmallBottom,
}

/// A pane: its fixed class and the view currently occupying it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pane {
    pub class: PaneClass,
    pub view: Option<ViewType>,
}

/// Adaptive band layout: a full-height left big pane plus a right half that
/// is one big pane or two small panes, decided by `has_small_view`.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneLayout {
    pub big_left: Pane,
    pub big_right: Pane,
    pub small_top: Pane,
    pub small_bottom: Pane,
    /// Focused pane; `Tab`/`Shift+Tab` cycle it in tree order.
    pub focus: PaneId,
    /// Non-latching maximize: clears on `z`, on focus change, and on `Esc`.
    pub maximized: Option<PaneId>,
    pub main_split_ratio: f64,
    pub small_split_ratio: f64,
}

impl ViewType {
    /// Which pane class a view may occupy (spec "Window classes route views
    /// to panes").
    pub fn class(self) -> PaneClass {
        match self {
            ViewType::Graph | ViewType::Physical => PaneClass::Big,
            ViewType::Optimizer | ViewType::SourceViewer => PaneClass::Small,
        }
    }
}

impl PaneLayout {
    pub fn pane(&self, id: PaneId) -> &Pane {
        match id {
            PaneId::BigLeft => &self.big_left,
            PaneId::BigRight => &self.big_right,
            PaneId::SmallTop => &self.small_top,
            PaneId::SmallBottom => &self.small_bottom,
        }
    }

    pub fn pane_mut(&mut self, id: PaneId) -> &mut Pane {
        match id {
            PaneId::BigLeft => &mut self.big_left,
            PaneId::BigRight => &mut self.big_right,
            PaneId::SmallTop => &mut self.small_top,
            PaneId::SmallBottom => &mut self.small_bottom,
        }
    }

    /// Whether a small-class view is open: the right half shows the two small
    /// panes instead of a single big pane.
    pub fn has_small_view(&self) -> bool {
        self.small_top.view.is_some() || self.small_bottom.view.is_some()
    }
}

impl Default for PaneLayout {
    /// Startup arrangement (spec "Startup pane configuration"): the module UI
    /// (Physical) in the left big pane and the source viewer in a small pane.
    fn default() -> Self {
        PaneLayout {
            big_left: Pane {
                class: PaneClass::Big,
                view: Some(ViewType::Physical),
            },
            big_right: Pane {
                class: PaneClass::Big,
                view: None,
            },
            small_top: Pane {
                class: PaneClass::Small,
                view: Some(ViewType::SourceViewer),
            },
            small_bottom: Pane {
                class: PaneClass::Small,
                view: None,
            },
            focus: PaneId::BigLeft,
            maximized: None,
            main_split_ratio: 0.5,
            small_split_ratio: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, FocusSlot, Rect};
    use crate::patch::Patch;
    use std::path::Path;

    /// A 200x100 main band: the horizontal boundary lands at x=100 and the
    /// vertical one at y=50, so quarter/half rectangles are exact integers.
    fn band() -> Rect {
        Rect::new(0, 0, 200, 100)
    }

    /// An app with a real patch loaded (the optimizer's `g o` open needs
    /// sections to optimize).
    fn app_with_patch() -> App {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        assert!(app.load_patch(patch));
        app
    }

    /// Startup arrangement with the viewer closed and the graph in the right
    /// big pane: BigLeft = Panels, BigRight = Graph, no small view, focus on
    /// BigRight (the two-big-pane arrangement used by the transition tests).
    fn big_arrangement_graph_right() -> App {
        let mut app = app_with_patch();
        app.layout.focus = PaneId::SmallTop;
        app.tile_stack.focus = FocusSlot::Slot(0); // the viewer is slot 0
        app.close_focused_view(); // closes the startup viewer
        assert!(!app.layout.has_small_view());
        app.layout.focus = PaneId::BigRight;
        app.open_view(ViewType::Graph);
        assert_eq!(app.layout.big_right.view, Some(ViewType::Graph));
        assert_eq!(app.layout.focus, PaneId::BigRight);
        app
    }

    #[test]
    fn every_view_has_its_window_class() {
        assert_eq!(ViewType::Graph.class(), PaneClass::Big);
        assert_eq!(ViewType::Physical.class(), PaneClass::Big);
        assert_eq!(ViewType::Optimizer.class(), PaneClass::Small);
        assert_eq!(ViewType::SourceViewer.class(), PaneClass::Small);
    }

    #[test]
    fn default_layout_starts_in_small_arrangement() {
        let layout = PaneLayout::default();
        assert_eq!(layout.big_left.view, Some(ViewType::Physical));
        assert_eq!(layout.small_top.view, Some(ViewType::SourceViewer));
        assert_eq!(layout.big_right.view, None);
        assert_eq!(layout.small_bottom.view, None);
        assert_eq!(layout.focus, PaneId::BigLeft);
        assert!(layout.maximized.is_none());
        assert_eq!(layout.main_split_ratio, 0.5);
        assert_eq!(layout.small_split_ratio, 0.5);
        assert!(layout.has_small_view());
    }

    #[test]
    fn small_view_predicate_tracks_the_small_panes() {
        let mut layout = PaneLayout::default();
        layout.small_top.view = None;
        layout.small_bottom.view = None;
        assert!(!layout.has_small_view());
    }

    #[test]
    fn pane_accessors_route_by_id() {
        let mut layout = PaneLayout::default();
        assert_eq!(layout.pane(PaneId::BigLeft).class, PaneClass::Big);
        assert_eq!(layout.pane(PaneId::SmallTop).class, PaneClass::Small);
        layout.pane_mut(PaneId::SmallBottom).view = Some(ViewType::Optimizer);
        assert_eq!(
            layout.pane(PaneId::SmallBottom).view,
            Some(ViewType::Optimizer)
        );
    }

    #[test]
    fn cycle_focus_tabs_through_all_three_panes_of_the_small_arrangement() {
        // All three panes hold views: module UI (BigLeft), source viewer
        // (SmallTop), optimizer (SmallBottom). Tree order is BigLeft ->
        // SmallTop -> SmallBottom (spec "Tab cycles focus with two small
        // panes").
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        assert!(app.load_patch(patch));
        app.open_view(ViewType::Optimizer); // -> SmallBottom, focused
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::Optimizer));
        // Back to the left big pane, then Tab forward through all three.
        app.layout.focus = PaneId::BigLeft;
        app.tile_stack.focus = FocusSlot::Panels;
        app.cycle_focus(true);
        assert_eq!(app.layout.focus, PaneId::SmallTop);
        app.cycle_focus(true);
        assert_eq!(app.layout.focus, PaneId::SmallBottom);
        app.cycle_focus(true);
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        // Shift+Tab walks the same order backward.
        app.cycle_focus(false);
        assert_eq!(app.layout.focus, PaneId::SmallBottom);
    }

    #[test]
    fn cycle_focus_tabs_between_the_two_big_panes() {
        // Big arrangement: module UI left, graph right, no small panes (spec
        // "Tab cycles focus with two big panes").
        let mut app = App::new();
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.layout.big_right.view = Some(ViewType::Graph);
        app.tile_stack.slots = vec![ViewType::Graph];
        app.cycle_focus(true); // BigLeft -> BigRight
        assert_eq!(app.layout.focus, PaneId::BigRight);
        app.cycle_focus(true); // -> BigLeft
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        app.cycle_focus(false); // Shift+Tab back to BigRight
        assert_eq!(app.layout.focus, PaneId::BigRight);
    }

    #[test]
    fn cycle_focus_skips_the_empty_small_pane() {
        // Startup small arrangement: the second small pane is empty, so Tab
        // from the first small pane wraps to the left big pane (spec "Empty
        // panes are skipped").
        let mut app = App::new();
        app.layout.focus = PaneId::SmallTop;
        app.tile_stack.focus = FocusSlot::Slot(0);
        app.cycle_focus(true);
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        // Backward from BigLeft skips the empty pane as well.
        app.cycle_focus(false);
        assert_eq!(app.layout.focus, PaneId::SmallTop);
    }

    // ── Task 4.1: geometry in both arrangements and the maximize override ──

    #[test]
    fn geometry_small_arrangement_splits_the_right_half_vertically() {
        let app = App::new(); // startup: viewer open -> small arrangement
        assert_eq!(
            app.pane_geometry(band()),
            vec![
                (PaneId::BigLeft, Rect::new(0, 0, 100, 100)),
                (PaneId::SmallTop, Rect::new(100, 0, 100, 50)),
                (PaneId::SmallBottom, Rect::new(100, 50, 100, 50)),
            ]
        );
    }

    #[test]
    fn geometry_big_arrangement_is_two_equal_panes_side_by_side() {
        let mut app = App::new();
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        assert_eq!(
            app.pane_geometry(band()),
            vec![
                (PaneId::BigLeft, Rect::new(0, 0, 100, 100)),
                (PaneId::BigRight, Rect::new(100, 0, 100, 100)),
            ]
        );
    }

    #[test]
    fn geometry_respects_both_ratio_clamps() {
        let mut app = App::new();
        // Small arrangement, both ratios at the 0.7 ceiling.
        app.layout.main_split_ratio = 0.7;
        app.layout.small_split_ratio = 0.7;
        assert_eq!(
            app.pane_geometry(band()),
            vec![
                (PaneId::BigLeft, Rect::new(0, 0, 140, 100)),
                (PaneId::SmallTop, Rect::new(140, 0, 60, 70)),
                (PaneId::SmallBottom, Rect::new(140, 70, 60, 30)),
            ]
        );
        // Small arrangement, both ratios at the 0.3 floor (out-of-range clamps).
        // Out-of-range clamps: 0.1 -> 0.3 floor, 0.9 -> 0.7 ceiling.
        app.layout.main_split_ratio = 0.1;
        app.layout.small_split_ratio = 0.9;
        assert_eq!(
            app.pane_geometry(band()),
            vec![
                (PaneId::BigLeft, Rect::new(0, 0, 60, 100)),
                (PaneId::SmallTop, Rect::new(60, 0, 140, 70)),
                (PaneId::SmallBottom, Rect::new(60, 70, 140, 30)),
            ]
        );
        // Big arrangement: the small ratio is irrelevant, the main one clamps.
        app.layout.main_split_ratio = 0.7;
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        assert_eq!(
            app.pane_geometry(band()),
            vec![
                (PaneId::BigLeft, Rect::new(0, 0, 140, 100)),
                (PaneId::BigRight, Rect::new(140, 0, 60, 100)),
            ]
        );
    }

    #[test]
    fn geometry_maximize_returns_only_the_maximized_pane_at_full_band() {
        let mut app = App::new();
        app.layout.maximized = Some(PaneId::SmallBottom);
        assert_eq!(
            app.pane_geometry(band()),
            vec![(PaneId::SmallBottom, band())]
        );
        // The override wins in the big arrangement too.
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.layout.maximized = Some(PaneId::BigRight);
        assert_eq!(app.pane_geometry(band()), vec![(PaneId::BigRight, band())]);
    }

    // ── Class routing ──

    #[test]
    fn class_routing_places_each_view_in_a_pane_of_its_class() {
        let mut app = app_with_patch();
        app.open_view(ViewType::Optimizer);
        app.open_view(ViewType::Graph);
        for view in [ViewType::Graph, ViewType::Optimizer] {
            let id = app
                .pane_holding(view)
                .unwrap_or_else(|| panic!("{view:?} is open"));
            assert_eq!(
                app.layout.pane(id).class,
                view.class(),
                "{view:?} must sit in a pane of its own class"
            );
        }
    }

    #[test]
    fn opening_a_small_view_keeps_both_big_pane_views() {
        let mut app = app_with_patch();
        app.open_view(ViewType::Optimizer); // -> SmallBottom, focused
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::Optimizer));
        assert_eq!(
            app.layout.big_left.view,
            Some(ViewType::Physical),
            "the left big pane keeps its view"
        );
        assert_eq!(
            app.layout.small_top.view,
            Some(ViewType::SourceViewer),
            "the first small pane keeps its view"
        );
    }

    #[test]
    fn opening_a_visible_view_focuses_it_without_duplicating() {
        let mut app = App::new();
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        app.open_view(ViewType::SourceViewer); // already open at startup
        assert_eq!(app.layout.focus, PaneId::SmallTop);
        let copies = [
            PaneId::BigLeft,
            PaneId::BigRight,
            PaneId::SmallTop,
            PaneId::SmallBottom,
        ]
        .into_iter()
        .filter(|&id| app.layout.pane(id).view == Some(ViewType::SourceViewer))
        .count();
        assert_eq!(copies, 1, "no second copy is opened");
    }

    // ── Replace-on-open ──

    #[test]
    fn opening_into_an_occupied_big_pane_replaces_its_view() {
        let mut app = App::new();
        app.layout.focus = PaneId::BigLeft; // holds the module UI
        app.open_view(ViewType::Graph);
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        assert!(
            app.pane_holding(ViewType::Physical).is_none(),
            "the module UI is replaced, not shown twice"
        );
    }

    #[test]
    fn opening_into_an_occupied_small_pane_replaces_its_view() {
        let mut app = app_with_patch();
        app.cycle_focus(true); // BigLeft -> SmallTop (holds the source viewer)
        app.open_view(ViewType::Optimizer);
        assert_eq!(app.layout.small_top.view, Some(ViewType::Optimizer));
        assert!(
            app.pane_holding(ViewType::SourceViewer).is_none(),
            "the source viewer is replaced"
        );
        assert!(!app.showing_viewer);
    }

    // ── Physical and module UI are mutually exclusive ──

    // Physical IS the module UI now - no separate Panels view

    // Physical IS the module UI now - no separate Panels view

    // Physical IS the module UI - no coexistence issue possible

    // ── Arrangement transitions ──

    #[test]
    fn first_small_view_repurposes_the_right_half() {
        let mut app = big_arrangement_graph_right();
        app.open_view(ViewType::SourceViewer);
        assert!(app.layout.has_small_view(), "the right half splits");
        assert!(
            app.pane_holding(ViewType::Graph).is_none(),
            "the big view in the right half closes"
        );
        assert_eq!(app.layout.big_right.view, None);
        assert_eq!(app.layout.small_top.view, Some(ViewType::SourceViewer));
        assert!(
            app.status_message.contains("Right half split"),
            "the repurposed pane is reported: {}",
            app.status_message
        );
    }

    #[test]
    fn opening_the_optimizer_repurposes_the_right_half() {
        let mut app = big_arrangement_graph_right();
        assert!(app.open_optimizer());
        assert!(app.layout.has_small_view(), "the right half splits");
        assert!(app.pane_holding(ViewType::Graph).is_none());
        assert_eq!(app.layout.big_right.view, None);
        assert_eq!(app.layout.small_top.view, Some(ViewType::Optimizer));
    }

    #[test]
    fn closing_the_last_small_view_restores_the_big_right_pane() {
        let mut app = App::new(); // the viewer is the only small-class view
        assert!(app.layout.has_small_view());
        app.layout.focus = PaneId::SmallTop;
        app.tile_stack.focus = FocusSlot::Slot(0);
        app.close_focused_view();
        assert!(!app.layout.has_small_view());
        assert_eq!(app.layout.small_top.view, None);
        assert_eq!(
            app.pane_geometry(band()),
            vec![
                (PaneId::BigLeft, Rect::new(0, 0, 100, 100)),
                (PaneId::BigRight, Rect::new(100, 0, 100, 100)),
            ],
            "the right half returns to a single big pane"
        );
    }

    // ── Non-latching maximize ──

    #[test]
    fn maximize_toggle_marks_and_restores_the_focused_pane() {
        let mut app = App::new();
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        app.maximize_toggle();
        assert_eq!(app.layout.maximized, Some(PaneId::BigLeft));
        assert_eq!(app.status_message, "Maximized");
        app.maximize_toggle();
        assert_eq!(app.layout.maximized, None);
        assert_eq!(app.status_message, "Layout restored");
    }

    #[test]
    fn maximize_clears_on_a_tab_focus_change() {
        let mut app = App::new();
        app.maximize_toggle();
        assert_eq!(app.layout.maximized, Some(PaneId::BigLeft));
        app.cycle_focus(true); // -> SmallTop
        assert_eq!(app.layout.maximized, None, "focus change clears maximize");
        assert_eq!(app.layout.focus, PaneId::SmallTop);
    }

    #[test]
    fn maximize_clears_on_esc_before_closing_the_view() {
        let mut app = App::new();
        app.cycle_focus(true); // BigLeft -> SmallTop
        app.maximize_toggle();
        assert_eq!(app.layout.maximized, Some(PaneId::SmallTop));
        app.close_focused_view();
        assert_eq!(app.layout.maximized, None);
        assert_eq!(
            app.layout.small_top.view,
            Some(ViewType::SourceViewer),
            "Esc clears the maximize first and keeps the view open"
        );
    }

    // ── Same-class swap ──

    #[test]
    fn swap_big_exchanges_the_two_big_panes_and_preserves_focus() {
        let mut app = App::new();
        app.layout.big_right.view = Some(ViewType::Graph);
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.tile_stack.slots = vec![ViewType::Graph];
        app.layout.focus = PaneId::BigRight;
        app.tile_stack.focus = FocusSlot::Slot(0);
        app.swap_big();
        assert_eq!(app.layout.big_left.view, Some(ViewType::Graph));
        assert_eq!(app.layout.big_right.view, Some(ViewType::Physical));
        assert_eq!(app.layout.focus, PaneId::BigRight, "focus preserved");
    }

    #[test]
    fn swap_big_promotes_a_small_pane_view_into_the_big_pane() {
        let mut app = App::new();
        app.cycle_focus(true); // BigLeft -> SmallTop (holds the viewer)
        assert_eq!(app.layout.focus, PaneId::SmallTop);
        app.swap_big();
        assert_eq!(app.layout.big_left.view, Some(ViewType::SourceViewer));
        assert_eq!(app.layout.small_top.view, Some(ViewType::Physical));
        assert_eq!(app.layout.focus, PaneId::SmallTop, "focus preserved");
    }

    #[test]
    fn swap_big_is_a_noop_when_the_only_big_pane_is_focused() {
        let mut app = App::new();
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        let before = app.layout.clone();
        app.swap_big();
        assert_eq!(app.layout, before, "layout unchanged");
        assert!(app.status_message.contains("No swap applies"));
    }

    #[test]
    fn swap_small_exchanges_the_two_small_panes_and_preserves_focus() {
        let mut app = app_with_patch();
        app.open_view(ViewType::Optimizer); // -> SmallBottom, focused
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::Optimizer));
        app.swap_small();
        assert_eq!(app.layout.small_top.view, Some(ViewType::Optimizer));
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::SourceViewer));
        assert_eq!(app.layout.focus, PaneId::SmallBottom, "focus preserved");
    }

    #[test]
    fn swap_small_is_a_noop_when_no_small_panes_are_shown() {
        let mut app = App::new();
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.tile_stack.slots = Vec::new();
        let before = app.layout.clone();
        app.swap_small();
        assert_eq!(app.layout, before, "layout unchanged");
        assert!(app.status_message.contains("No swap applies"));
    }

    #[test]
    fn swap_small_moves_a_view_into_the_empty_pane() {
        let mut app = App::new();
        assert!(app.layout.small_bottom.view.is_none());
        app.swap_small();
        assert_eq!(app.layout.small_top.view, None);
        assert_eq!(app.layout.small_bottom.view, Some(ViewType::SourceViewer));
    }

    // ── Focus cycling with an empty pane ──

    #[test]
    fn cycle_focus_is_a_noop_with_a_single_filled_pane() {
        let mut app = App::new();
        // Big arrangement with only the module UI open.
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.tile_stack.slots = Vec::new();
        app.layout.focus = PaneId::BigLeft;
        app.cycle_focus(true);
        assert_eq!(app.layout.focus, PaneId::BigLeft);
        app.cycle_focus(false);
        assert_eq!(app.layout.focus, PaneId::BigLeft);
    }

    #[test]
    fn cycle_focus_from_an_empty_pane_advances_to_a_filled_pane() {
        let mut app = app_with_patch();
        // Fill both big and a small pane, then close the startup viewer so
        // SmallTop is empty while BigLeft and SmallBottom still hold views.
        app.open_view(ViewType::Optimizer); // -> SmallBottom, focused
        app.cycle_focus(true); // SmallBottom -> BigLeft
        app.cycle_focus(true); // BigLeft -> SmallTop (the viewer)
        assert_eq!(app.layout.focus, PaneId::SmallTop);
        app.close_focused_view();
        assert_eq!(app.layout.focus, PaneId::SmallTop);
        assert_eq!(app.layout.small_top.view, None);
        app.cycle_focus(true);
        assert_eq!(
            app.layout.focus,
            PaneId::SmallBottom,
            "Tab from an empty pane reaches a filled one"
        );
    }
}
